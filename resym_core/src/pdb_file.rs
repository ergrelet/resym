use dashmap::DashMap;
#[cfg(target_arch = "wasm32")]
use instant::Instant;
use pdb::FallibleIterator;
#[cfg(feature = "rayon")]
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use std::{
    collections::BTreeSet,
    io::{self, Read, Seek},
    path::PathBuf,
    sync::Arc,
};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs::File, path::Path, time::Instant};

use crate::{
    cond_par_iter,
    error::{Result, ResymCoreError},
    frontend::ModuleList,
    pdb_types::{
        self, is_unnamed_type, type_name, DataFormatConfiguration, PrimitiveReconstructionFlavor,
    },
};

/// Wrapper for different buffer types processed by `resym`
#[derive(Debug)]
pub enum PDBDataSource {
    File(std::fs::File),
    Vec(io::Cursor<Vec<u8>>),
    SharedArray(io::Cursor<Arc<[u8]>>),
}

impl Seek for PDBDataSource {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match self {
            PDBDataSource::File(file) => file.seek(pos),
            PDBDataSource::Vec(vec) => vec.seek(pos),
            PDBDataSource::SharedArray(array) => array.seek(pos),
        }
    }
}

impl Read for PDBDataSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            PDBDataSource::File(file) => file.read(buf),
            PDBDataSource::Vec(vec) => vec.read(buf),
            PDBDataSource::SharedArray(array) => array.read(buf),
        }
    }
}

pub struct PdbFile<'p, T>
where
    T: io::Seek + io::Read + 'p,
{
    pub complete_type_list: Vec<(String, pdb::TypeIndex)>,
    pub forwarder_to_complete_type: Arc<DashMap<pdb::TypeIndex, pdb::TypeIndex>>,
    pub machine_type: pdb::MachineType,
    pub type_information: pdb::TypeInformation<'p>,
    pub debug_information: pdb::DebugInformation<'p>,
    pub file_path: PathBuf,
    _pdb: pdb::PDB<'p, T>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'p> PdbFile<'p, File> {
    /// Create `PdbFile` from an `std::path::Path`
    pub fn load_from_file(pdb_file_path: &Path) -> Result<PdbFile<'p, PDBDataSource>> {
        let file = PDBDataSource::File(File::open(pdb_file_path)?);
        let mut pdb = pdb::PDB::open(file)?;
        let type_information = pdb.type_information()?;
        let debug_information = pdb.debug_information()?;
        let machine_type = pdb.debug_information()?.machine_type()?;

        let mut pdb_file = PdbFile {
            complete_type_list: vec![],
            forwarder_to_complete_type: Arc::new(DashMap::default()),
            machine_type,
            type_information,
            debug_information,
            file_path: pdb_file_path.to_owned(),
            _pdb: pdb,
        };
        pdb_file.load_symbols()?;

        Ok(pdb_file)
    }
}

impl<'p> PdbFile<'p, PDBDataSource> {
    /// Create `PdbFile` from a `String` and a `Vec<u8>`
    pub fn load_from_bytes_as_vec(
        pdb_file_name: String,
        pdb_file_data: Vec<u8>,
    ) -> Result<PdbFile<'p, PDBDataSource>> {
        let reader = PDBDataSource::Vec(io::Cursor::new(pdb_file_data));
        let mut pdb = pdb::PDB::open(reader)?;
        let type_information = pdb.type_information()?;
        let debug_information = pdb.debug_information()?;
        let machine_type = pdb.debug_information()?.machine_type()?;

        let mut pdb_file = PdbFile {
            complete_type_list: vec![],
            forwarder_to_complete_type: Arc::new(DashMap::default()),
            machine_type,
            type_information,
            debug_information,
            file_path: pdb_file_name.into(),
            _pdb: pdb,
        };
        pdb_file.load_symbols()?;

        Ok(pdb_file)
    }

    /// Create `PdbFile` from a `String` and a `Arc<[u8]>`
    pub fn load_from_bytes_as_array(
        pdb_file_name: String,
        pdb_file_data: Arc<[u8]>,
    ) -> Result<PdbFile<'p, PDBDataSource>> {
        let reader = PDBDataSource::SharedArray(io::Cursor::new(pdb_file_data));
        let mut pdb = pdb::PDB::open(reader)?;
        let type_information = pdb.type_information()?;
        let debug_information = pdb.debug_information()?;
        let machine_type = pdb.debug_information()?.machine_type()?;

        let mut pdb_file = PdbFile {
            complete_type_list: vec![],
            forwarder_to_complete_type: Arc::new(DashMap::default()),
            machine_type,
            type_information,
            debug_information,
            file_path: pdb_file_name.into(),
            _pdb: pdb,
        };
        pdb_file.load_symbols()?;

        Ok(pdb_file)
    }
}

impl<'p, T> PdbFile<'p, T>
where
    T: io::Seek + io::Read + std::fmt::Debug + 'p,
{
    fn load_symbols(&mut self) -> Result<()> {
        // Build the list of complete types
        let complete_symbol_map: DashMap<String, pdb::TypeIndex> = DashMap::default();
        let mut forwarders = vec![];
        let pdb_start = Instant::now();

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
        let fwd_start = Instant::now();
        cond_par_iter!(forwarders).for_each(|(fwd_name, fwd_type_id)| {
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
                primitives_flavor,
                reconstruct_dependencies,
                print_access_specifiers,
            )
        }
    }

    pub fn reconstruct_type_by_type_index(
        &self,
        type_index: pdb::TypeIndex,
        primitives_flavor: PrimitiveReconstructionFlavor,
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

    pub fn module_list(&self) -> Result<ModuleList> {
        let module_list = self
            .debug_information
            .modules()?
            .enumerate()
            .map(|(index, module)| Ok((module.module_name().into_owned(), index)));

        Ok(module_list.collect()?)
    }

    pub fn reconstruct_module_by_path(
        &mut self,
        module_path: &str,
        primitives_flavor: PrimitiveReconstructionFlavor,
    ) -> Result<String> {
        // Find index for module
        let mut modules = self.debug_information.modules()?;
        let module_index = modules.position(|module| Ok(module.module_name() == module_path))?;

        match module_index {
            None => Err(ResymCoreError::ModuleNotFoundError(format!(
                "Module '{}' not found",
                module_path
            ))),
            Some(module_index) => self.reconstruct_module_by_index(module_index, primitives_flavor),
        }
    }

    pub fn reconstruct_module_by_index(
        &mut self,
        module_index: usize,
        primitives_flavor: PrimitiveReconstructionFlavor,
    ) -> Result<String> {
        let mut modules = self.debug_information.modules()?;
        let module = modules.nth(module_index)?.ok_or_else(|| {
            ResymCoreError::ModuleInfoNotFoundError(format!("Module #{} not found", module_index))
        })?;

        let module_info = self._pdb.module_info(&module)?.ok_or_else(|| {
            ResymCoreError::ModuleInfoNotFoundError(format!(
                "No module information present for '{}'",
                module.object_file_name()
            ))
        })?;

        // Populate our `TypeFinder`
        let mut type_finder = self.type_information.finder();
        {
            let mut type_iter = self.type_information.iter();
            while (type_iter.next()?).is_some() {
                type_finder.update(&type_iter);
            }
        }

        let mut result = String::default();
        module_info.symbols()?.for_each(|symbol| {
            let mut needed_types = pdb_types::TypeSet::new();

            match symbol.parse()? {
                pdb::SymbolData::UserDefinedType(udt) => {
                    if let Ok(type_name) = type_name(
                        &type_finder,
                        &self.forwarder_to_complete_type,
                        udt.type_index,
                        &primitives_flavor,
                        &mut needed_types,
                    ) {
                        if type_name.0 == "..." {
                            // No type
                            result +=
                                format!("{}; // Missing type information\n", udt.name).as_str();
                        } else {
                            result +=
                                format!("using {} = {}{};\n", udt.name, type_name.0, type_name.1)
                                    .as_str();
                        }
                    }
                }
                pdb::SymbolData::Procedure(procedure) => {
                    if let Ok(type_name) = type_name(
                        &type_finder,
                        &self.forwarder_to_complete_type,
                        procedure.type_index,
                        &primitives_flavor,
                        &mut needed_types,
                    ) {
                        if type_name.0 == "..." {
                            // No type
                            result += format!(
                                "void {}(); // CodeSize={} (missing type information)\n",
                                procedure.name, procedure.len
                            )
                            .as_str();
                        } else {
                            result += format!(
                                "{}{}{}; // CodeSize={}\n",
                                type_name.0, procedure.name, type_name.1, procedure.len
                            )
                            .as_str();
                        }
                    }
                }
                pdb::SymbolData::Data(data) => {
                    if let Ok(type_name) = type_name(
                        &type_finder,
                        &self.forwarder_to_complete_type,
                        data.type_index,
                        &primitives_flavor,
                        &mut needed_types,
                    ) {
                        if type_name.0 == "..." {
                            // No type
                            result +=
                                format!("{}; // Missing type information\n", data.name).as_str();
                        } else {
                            result +=
                                format!("{} {}{};\n", type_name.0, data.name, type_name.1).as_str();
                        }
                    }
                }
                pdb::SymbolData::UsingNamespace(namespace) => {
                    result += format!("using namespace {};\n", namespace.name).as_str();
                }
                pdb::SymbolData::AnnotationReference(annotation) => {
                    // TODO(ergrelet): update when support for annotations
                    // (symbol kind 0x1019) has been implemented in `pdb`
                    result += format!("__annotation(); // {}\n", annotation.name).as_str();
                }
                // Ignore
                _ => {}
            }

            Ok(())
        })?;

        Ok(result)
    }

    fn reconstruct_type_by_type_index_internal(
        &self,
        type_finder: &pdb::TypeFinder,
        type_index: pdb::TypeIndex,
        primitives_flavor: PrimitiveReconstructionFlavor,
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
            &primitives_flavor,
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
        let dep_start = Instant::now();
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
                        &primitives_flavor,
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

    pub fn reconstruct_all_types(
        &self,
        primitives_flavor: PrimitiveReconstructionFlavor,
        print_access_specifiers: bool,
    ) -> Result<String> {
        let mut type_data = pdb_types::Data::new();

        let mut type_finder = self.type_information.finder();
        {
            // Populate our `TypeFinder`
            let mut type_iter = self.type_information.iter();
            while (type_iter.next()?).is_some() {
                type_finder.update(&type_iter);
            }

            // Add the requested types
            type_iter = self.type_information.iter();
            while let Some(item) = type_iter.next()? {
                let mut needed_types = pdb_types::TypeSet::new();
                let type_index = item.index();
                let result = type_data.add(
                    &type_finder,
                    &self.forwarder_to_complete_type,
                    type_index,
                    &primitives_flavor,
                    &mut needed_types,
                );
                if let Err(err) = result {
                    match err {
                        ResymCoreError::PdbError(err) => {
                            // Ignore this kind of error since some particular PDB features might not be supported.
                            // This allows the recontruction to go through with the correctly reconstructed types.
                            log::warn!("Failed to reconstruct type with index {type_index}: {err}")
                        }
                        _ => return Err(err),
                    }
                }
            }
        }

        let fmt_configuration = DataFormatConfiguration {
            print_access_specifiers,
        };
        let mut reconstruction_output = String::new();
        type_data.reconstruct(&fmt_configuration, &mut reconstruction_output)?;
        Ok(reconstruction_output)
    }
}
