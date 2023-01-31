use dashmap::DashMap;
use pdb::FallibleIterator;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use std::{
    collections::BTreeSet,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::error::{Result, ResymCoreError};
use crate::pdb_types::{
    self, is_unnamed_type, DataFormatConfiguration, PrimitiveReconstructionFlavor,
};

pub struct PdbFile<'p> {
    pub complete_type_list: Vec<(String, pdb::TypeIndex)>,
    pub forwarder_to_complete_type: Arc<DashMap<pdb::TypeIndex, pdb::TypeIndex>>,
    pub machine_type: pdb::MachineType,
    pub type_information: pdb::TypeInformation<'p>,
    pub file_path: PathBuf,
    _pdb: pdb::PDB<'p, File>,
}

impl<'p> PdbFile<'p> {
    pub fn load_from_file(pdb_file_path: &Path) -> Result<PdbFile<'p>> {
        let file = File::open(pdb_file_path)?;
        let mut pdb = pdb::PDB::open(file)?;
        let type_information = pdb.type_information()?;
        let machine_type = pdb.debug_information()?.machine_type()?;

        let mut pdb_file = PdbFile {
            complete_type_list: vec![],
            forwarder_to_complete_type: Arc::new(DashMap::default()),
            machine_type,
            type_information,
            file_path: pdb_file_path.to_owned(),
            _pdb: pdb,
        };
        pdb_file.load_symbols()?;

        Ok(pdb_file)
    }

    fn load_symbols(&mut self) -> Result<()> {
        // Build the list of complete types
        let complete_symbol_map: DashMap<String, pdb::TypeIndex> = DashMap::default();
        let mut forwarders = vec![];
        let pdb_start = std::time::Instant::now();

        let mut type_finder = self.type_information.finder();
        let mut type_info_iter = self.type_information.iter();
        while let Some(type_info) = type_info_iter.next()? {
            // keep building the index
            type_finder.update(&type_info_iter);

            let type_index = type_info.index();
            if let Ok(type_data) = type_info.parse() {
                match type_data {
                    pdb::TypeData::Class(data) => {
                        let mut class_name = data.name.to_string().into_owned();

                        // Ignore forward references
                        if data.properties.forward_reference() {
                            forwarders.push((class_name, type_index));
                            continue;
                        }
                        complete_symbol_map.insert(class_name.clone(), type_index);

                        // Rename anonymous tags to something unique
                        if is_unnamed_type(&class_name) {
                            class_name = format!("_unnamed_{type_index}");
                        }
                        self.complete_type_list.push((class_name, type_index));
                    }
                    pdb::TypeData::Union(data) => {
                        let mut class_name = data.name.to_string().into_owned();

                        // Ignore forward references
                        if data.properties.forward_reference() {
                            forwarders.push((class_name, type_index));
                            continue;
                        }
                        complete_symbol_map.insert(class_name.clone(), type_index);

                        // Rename anonymous tags to something unique
                        if is_unnamed_type(&class_name) {
                            class_name = format!("_unnamed_{type_index}");
                        }
                        self.complete_type_list.push((class_name, type_index));
                    }
                    pdb::TypeData::Enumeration(data) => {
                        let mut class_name = data.name.to_string().into_owned();

                        // Ignore forward references
                        if data.properties.forward_reference() {
                            forwarders.push((class_name, type_index));
                            continue;
                        }
                        complete_symbol_map.insert(class_name.clone(), type_index);

                        // Rename anonymous tags to something unique
                        if is_unnamed_type(&class_name) {
                            class_name = format!("_unnamed_{type_index}");
                        }
                        self.complete_type_list.push((class_name, type_index));
                    }
                    _ => {}
                }
            }
        }
        log::debug!("PDB loading took {} ms", pdb_start.elapsed().as_millis());

        // Resolve forwarder references to their corresponding complete type, in parallel
        let fwd_start = std::time::Instant::now();
        forwarders.par_iter().for_each(|(fwd_name, fwd_type_id)| {
            if let Some(complete_type_index) = complete_symbol_map.get(fwd_name) {
                self.forwarder_to_complete_type
                    .insert(*fwd_type_id, *complete_type_index);
            } else {
                log::debug!("'{}''s type definition wasn't found", fwd_name);
            }
        });
        log::debug!(
            "Forwarder resolution took {} ms",
            fwd_start.elapsed().as_millis()
        );

        Ok(())
    }

    pub fn reconstruct_type_by_name(
        &self,
        type_name: &str,
        primitives_flavor: PrimitiveReconstructionFlavor,
        reconstruct_dependencies: bool,
        print_access_specifiers: bool,
    ) -> Result<String> {
        // Populate our `TypeFinder` and find the right type index
        let mut type_index = pdb::TypeIndex::default();
        let mut type_finder = self.type_information.finder();
        {
            let mut type_iter = self.type_information.iter();
            while let Some(item) = type_iter.next()? {
                type_finder.update(&type_iter);

                let item_type_index = item.index();
                if let Ok(type_data) = item.parse() {
                    match type_data {
                        pdb::TypeData::Class(data) => {
                            if data.properties.forward_reference() {
                                // Ignore incomplete type
                                continue;
                            }

                            // Rename anonymous tags to something unique
                            let class_name = data.name.to_string();
                            if is_unnamed_type(&class_name) {
                                if type_name == format!("_unnamed_{item_type_index}") {
                                    type_index = item_type_index;
                                }
                            } else if class_name == type_name {
                                type_index = item_type_index;
                            } else if let Some(unique_name) = data.unique_name {
                                if unique_name.to_string() == type_name {
                                    type_index = item_type_index;
                                }
                            }
                        }
                        pdb::TypeData::Union(data) => {
                            if data.properties.forward_reference() {
                                // Ignore incomplete type
                                continue;
                            }

                            // Rename anonymous tags to something unique
                            let union_name = data.name.to_string();
                            if is_unnamed_type(&union_name) {
                                if type_name == format!("_unnamed_{item_type_index}") {
                                    type_index = item_type_index;
                                }
                            } else if data.name.to_string() == type_name {
                                type_index = item_type_index;
                            } else if let Some(unique_name) = data.unique_name {
                                if unique_name.to_string() == type_name {
                                    type_index = item_type_index;
                                }
                            }
                        }
                        pdb::TypeData::Enumeration(data) => {
                            if data.properties.forward_reference() {
                                // Ignore incomplete type
                                continue;
                            }

                            // Rename anonymous tags to something unique
                            let enum_name = data.name.to_string();
                            if is_unnamed_type(&enum_name) {
                                if type_name == format!("_unnamed_{item_type_index}") {
                                    type_index = item_type_index;
                                }
                            } else if data.name.to_string() == type_name {
                                type_index = item_type_index;
                            } else if let Some(unique_name) = data.unique_name {
                                if unique_name.to_string() == type_name {
                                    type_index = item_type_index;
                                }
                            }
                        }
                        // Ignore
                        _ => {}
                    }
                }
            }
        }

        if type_index == pdb::TypeIndex::default() {
            Err(ResymCoreError::TypeNameNotFoundError(type_name.to_owned()))
        } else {
            self.reconstruct_type_by_type_index_internal(
                &type_finder,
                type_index,
                &primitives_flavor,
                reconstruct_dependencies,
                print_access_specifiers,
            )
        }
    }

    pub fn reconstruct_type_by_type_index(
        &self,
        type_index: pdb::TypeIndex,
        primitives_flavor: &PrimitiveReconstructionFlavor,
        reconstruct_dependencies: bool,
        print_access_specifiers: bool,
    ) -> Result<String> {
        // Populate our `TypeFinder`
        let mut type_finder = self.type_information.finder();
        {
            let mut type_iter = self.type_information.iter();
            while (type_iter.next()?).is_some() {
                type_finder.update(&type_iter);
            }
        }

        self.reconstruct_type_by_type_index_internal(
            &type_finder,
            type_index,
            primitives_flavor,
            reconstruct_dependencies,
            print_access_specifiers,
        )
    }

    fn reconstruct_type_by_type_index_internal(
        &self,
        type_finder: &pdb::TypeFinder,
        type_index: pdb::TypeIndex,
        primitives_flavor: &PrimitiveReconstructionFlavor,
        reconstruct_dependencies: bool,
        print_access_specifiers: bool,
    ) -> Result<String> {
        let fmt_configuration = DataFormatConfiguration {
            print_access_specifiers,
        };
        let mut type_data = pdb_types::Data::new();
        let mut needed_types = pdb_types::TypeSet::new();

        // Add the requested type first
        type_data.add(
            type_finder,
            &self.forwarder_to_complete_type,
            type_index,
            primitives_flavor,
            &mut needed_types,
        )?;

        // If dependencies aren't needed, we're done
        if !reconstruct_dependencies {
            let mut reconstruction_output = String::new();
            type_data.reconstruct(&fmt_configuration, &mut reconstruction_output)?;
            return Ok(reconstruction_output);
        }

        // Add all the needed types iteratively until we're done
        let mut dependencies_data = pdb_types::Data::new();
        let mut processed_types = BTreeSet::from([type_index]);
        let dep_start = std::time::Instant::now();
        loop {
            // Get the first element in needed_types without holding an immutable borrow
            let first = needed_types.difference(&processed_types).next().copied();
            match first {
                None => break,
                Some(needed_type_index) => {
                    // Add the type
                    dependencies_data.add(
                        type_finder,
                        &self.forwarder_to_complete_type,
                        needed_type_index,
                        primitives_flavor,
                        &mut needed_types,
                    )?;

                    processed_types.insert(needed_type_index);
                }
            }
        }
        log::debug!(
            "Dependencies reconstruction took {} ms",
            dep_start.elapsed().as_millis()
        );

        let mut reconstruction_output = String::new();
        dependencies_data.reconstruct(&fmt_configuration, &mut reconstruction_output)?;
        type_data.reconstruct(&fmt_configuration, &mut reconstruction_output)?;
        Ok(reconstruction_output)
    }
}
