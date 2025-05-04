use dashmap::DashMap;
#[cfg(target_arch = "wasm32")]
use instant::Instant;
use pdb::{AddressMap, FallibleIterator};
#[cfg(feature = "rayon")]
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use std::{
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque},
    fmt::Write,
    io::{self, Read, Seek},
    path::PathBuf,
    sync::{Arc, RwLock},
};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs::File, path::Path, time::Instant};

use crate::{
    error::{Result, ResymCoreError},
    frontend::ReconstructedType,
    par_iter_if_available,
    pdb_types::{
        self, is_unnamed_type, type_name, DataFormatConfiguration, PrimitiveReconstructionFlavor,
    },
};

// Type
pub type TypeIndex = u32;
/// Minimal symbol info (used by frontends)
pub type TypeInfo = (String, TypeIndex);
pub type TypeList = Vec<TypeInfo>;
/// Extended symbol info (used by the backend)
pub type TypeInfoEx = (String, TypeIndex, TypeKind);
pub type TypeListEx = Vec<TypeInfoEx>;
pub type TypeListExView<'t> = Vec<&'t TypeInfoEx>;
#[derive(Eq, PartialEq)]
pub enum TypeKind {
    Class,
    Union,
    Enum,
    Unknown,
}

// Symbol
/// `SymbolIndex` have two parts: a module index and a symbol index
pub type SymbolIndex = (ModuleIndex, u32);
/// Minimal symbol info (used by frontends)
pub type SymbolInfo = (String, SymbolIndex);
pub type SymbolList = Vec<SymbolInfo>;
/// Extended symbol info (used by the backend)
pub type SymbolInfoEx = (String, SymbolIndex, SymbolKind);
pub type SymbolListEx = Vec<SymbolInfoEx>;
pub type SymbolListExView<'t> = Vec<&'t SymbolInfoEx>;
#[derive(Eq, PartialEq)]
pub enum SymbolKind {
    Function,
    Variable,
    Type,
    Unknown,
}

// Module
pub type ModuleIndex = usize;
pub type ModuleInfo = (String, ModuleIndex);
pub type ModuleList = Vec<ModuleInfo>;

const GLOBAL_MODULE_INDEX: usize = usize::MAX;

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

/// Struct used in binary heaps, to prioritize certain symbol kind over others
#[derive(PartialEq, Eq)]
struct PrioritizedSymbol {
    priority: u16,
    name: String,
    index: SymbolIndex,
    kind: SymbolKind,
}

impl PartialOrd for PrioritizedSymbol {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedSymbol {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

pub struct PdbFile<'p, T>
where
    T: io::Seek + io::Read + 'p,
{
    pub complete_type_list: TypeListEx,
    pub forwarder_to_complete_type: Arc<DashMap<pdb::TypeIndex, pdb::TypeIndex>>,
    pub symbol_list: SymbolListEx,
    pub machine_type: pdb::MachineType,
    pub type_information: pdb::TypeInformation<'p>,
    pub debug_information: pdb::DebugInformation<'p>,
    pub global_symbols: pdb::SymbolTable<'p>,
    pub sections: Vec<pdb::ImageSectionHeader>,
    pub address_map: pdb::AddressMap<'p>,
    pub file_path: PathBuf,
    pub xref_to_map: RwLock<DashMap<TypeIndex, Vec<TypeIndex>>>,
    pdb: RwLock<pdb::PDB<'p, T>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'p> PdbFile<'p, File> {
    /// Create `PdbFile` from an `std::path::Path`
    pub fn load_from_file(pdb_file_path: &Path) -> Result<PdbFile<'p, PDBDataSource>> {
        let file = PDBDataSource::File(File::open(pdb_file_path)?);
        let mut pdb = pdb::PDB::open(file)?;
        let type_information = pdb.type_information()?;
        let debug_information = pdb.debug_information()?;
        let global_symbols = pdb.global_symbols()?;
        let sections = pdb.sections().unwrap_or_default().unwrap_or_default();
        let address_map = pdb.address_map()?;
        let machine_type = pdb.debug_information()?.machine_type()?;

        let mut pdb_file = PdbFile {
            complete_type_list: Default::default(),
            forwarder_to_complete_type: Arc::new(DashMap::default()),
            symbol_list: Default::default(),
            machine_type,
            type_information,
            debug_information,
            global_symbols,
            sections,
            address_map,
            file_path: pdb_file_path.to_owned(),
            xref_to_map: DashMap::default().into(),
            pdb: pdb.into(),
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
        let global_symbols = pdb.global_symbols()?;
        let sections = pdb.sections().unwrap_or_default().unwrap_or_default();
        let address_map = pdb.address_map()?;
        let machine_type = pdb.debug_information()?.machine_type()?;

        let mut pdb_file = PdbFile {
            complete_type_list: Default::default(),
            forwarder_to_complete_type: Arc::new(DashMap::default()),
            symbol_list: Default::default(),
            machine_type,
            type_information,
            debug_information,
            global_symbols,
            sections,
            address_map,
            file_path: pdb_file_name.into(),
            xref_to_map: DashMap::default().into(),
            pdb: pdb.into(),
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
        let global_symbols = pdb.global_symbols()?;
        let sections = pdb.sections().unwrap_or_default().unwrap_or_default();
        let address_map = pdb.address_map()?;
        let machine_type = pdb.debug_information()?.machine_type()?;

        let mut pdb_file = PdbFile {
            complete_type_list: Default::default(),
            forwarder_to_complete_type: Arc::new(DashMap::default()),
            symbol_list: Default::default(),
            machine_type,
            type_information,
            debug_information,
            global_symbols,
            sections,
            address_map,
            file_path: pdb_file_name.into(),
            xref_to_map: DashMap::default().into(),
            pdb: pdb.into(),
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
                        self.complete_type_list
                            .push((class_name, type_index.0, TypeKind::Class));
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
                        self.complete_type_list
                            .push((class_name, type_index.0, TypeKind::Union));
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
                        self.complete_type_list
                            .push((class_name, type_index.0, TypeKind::Enum));
                    }
                    _ => {}
                }
            }
        }
        log::debug!("PDB loading took {} ms", pdb_start.elapsed().as_millis());

        // Resolve forwarder references to their corresponding complete type, in parallel
        let fwd_start = Instant::now();
        par_iter_if_available!(forwarders).for_each(|(fwd_name, fwd_type_id)| {
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
        integers_as_hexadecimal: bool,
        ignore_std_types: bool,
    ) -> Result<ReconstructedType> {
        // Populate our `TypeFinder` and find the right type index
        let mut type_index = TypeIndex::default();
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
                                    type_index = item_type_index.0;
                                }
                            } else if class_name == type_name {
                                type_index = item_type_index.0;
                            } else if let Some(unique_name) = data.unique_name {
                                if unique_name.to_string() == type_name {
                                    type_index = item_type_index.0;
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
                                    type_index = item_type_index.0;
                                }
                            } else if data.name.to_string() == type_name {
                                type_index = item_type_index.0;
                            } else if let Some(unique_name) = data.unique_name {
                                if unique_name.to_string() == type_name {
                                    type_index = item_type_index.0;
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
                                    type_index = item_type_index.0;
                                }
                            } else if data.name.to_string() == type_name {
                                type_index = item_type_index.0;
                            } else if let Some(unique_name) = data.unique_name {
                                if unique_name.to_string() == type_name {
                                    type_index = item_type_index.0;
                                }
                            }
                        }
                        // Ignore
                        _ => {}
                    }
                }
            }
        }

        if type_index == TypeIndex::default() {
            Err(ResymCoreError::TypeNameNotFoundError(type_name.to_owned()))
        } else {
            self.reconstruct_type_by_type_index_internal(
                &type_finder,
                type_index,
                primitives_flavor,
                reconstruct_dependencies,
                print_access_specifiers,
                integers_as_hexadecimal,
                ignore_std_types,
            )
        }
    }

    pub fn reconstruct_type_by_index(
        &self,
        type_index: TypeIndex,
        primitives_flavor: PrimitiveReconstructionFlavor,
        reconstruct_dependencies: bool,
        print_access_specifiers: bool,
        integers_as_hexadecimal: bool,
        ignore_std_types: bool,
    ) -> Result<ReconstructedType> {
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
            integers_as_hexadecimal,
            ignore_std_types,
        )
    }

    pub fn type_list(&self) -> TypeListExView {
        self.complete_type_list.iter().collect()
    }

    pub fn symbol_list(&mut self) -> Result<SymbolListExView> {
        // If cache is populated, return the cached list
        if !self.symbol_list.is_empty() {
            return Ok(self.symbol_list.iter().collect());
        }

        let mut symbol_heap: BinaryHeap<PrioritizedSymbol> = BinaryHeap::new();

        // Modules' private symbols
        {
            let mut modules = self.debug_information.modules()?.enumerate();
            let mut pdb = self.pdb.write().expect("lock shouldn't be poisoned");
            while let Some((module_index, module)) = modules.next()? {
                let module_info = match pdb.module_info(&module)? {
                    Some(info) => info,
                    None => {
                        continue;
                    }
                };

                let mut module_symbols = module_info.symbols()?;
                while let Some(symbol) = module_symbols.next()? {
                    if let Ok(symbol_data) = symbol.parse() {
                        if let Some(symbol_name) = get_symbol_name(&symbol_data) {
                            symbol_heap.push(PrioritizedSymbol {
                                priority: symbol_priority(&symbol_data),
                                name: symbol_name.clone(),
                                index: (module_index, symbol.index().0),
                                kind: get_symbol_type(&symbol_data),
                            });
                        }
                    }
                }
            }
        }

        // Global symbols
        let mut symbol_table = self.global_symbols.iter();
        while let Some(symbol) = symbol_table.next()? {
            if let Ok(symbol_data) = symbol.parse() {
                if let Some(symbol_name) = get_symbol_name(&symbol_data) {
                    symbol_heap.push(PrioritizedSymbol {
                        priority: symbol_priority(&symbol_data),
                        name: symbol_name.clone(),
                        index: (GLOBAL_MODULE_INDEX, symbol.index().0),
                        kind: get_symbol_type(&symbol_data),
                    });
                }
            }
        }

        let mut symbol_names = HashSet::new();
        // Populate cache with result
        self.symbol_list = symbol_heap
            .into_sorted_vec()
            .into_iter()
            .filter_map(|s| {
                if !symbol_names.contains(&s.name) {
                    symbol_names.insert(s.name.clone());

                    Some((s.name, s.index, s.kind))
                } else {
                    None
                }
            })
            .collect();

        Ok(self.symbol_list.iter().collect())
    }

    pub fn module_list(&self) -> Result<ModuleList> {
        let module_list = self
            .debug_information
            .modules()?
            .enumerate()
            .map(|(index, module)| Ok((module.module_name().into_owned(), index)));

        Ok(module_list.collect()?)
    }

    pub fn reconstruct_symbol_by_index(
        &self,
        symbol_index: SymbolIndex,
        primitives_flavor: PrimitiveReconstructionFlavor,
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

        // Check which module the symbol is from
        if symbol_index.0 == GLOBAL_MODULE_INDEX {
            // Global symbols
            let mut symbol_table = self.global_symbols.iter();
            while let Some(symbol) = symbol_table.next()? {
                if symbol.index().0 == symbol_index.1 {
                    return Ok(self
                        .reconstruct_symbol(
                            &type_finder,
                            &symbol,
                            primitives_flavor,
                            print_access_specifiers,
                        )
                        .unwrap_or_default());
                }
            }
        } else if let Some(module) = self.debug_information.modules()?.nth(symbol_index.0)? {
            // Modules' private symbols
            let mut pdb = self.pdb.write().expect("lock shouldn't be poisoned");
            if let Some(module_info) = pdb.module_info(&module)? {
                let mut module_symbols = module_info.symbols_at(symbol_index.1.into())?;
                while let Some(symbol) = module_symbols.next()? {
                    if symbol.index().0 == symbol_index.1 {
                        return Ok(self
                            .reconstruct_symbol(
                                &type_finder,
                                &symbol,
                                primitives_flavor,
                                print_access_specifiers,
                            )
                            .unwrap_or_default());
                    }
                }
            }
        }

        Err(ResymCoreError::SymbolNotFoundError(format!(
            "Symbol #{:?} not found",
            symbol_index
        )))
    }

    pub fn reconstruct_symbol_by_name(
        &self,
        symbol_name: &str,
        primitives_flavor: PrimitiveReconstructionFlavor,
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

        // Global symbols
        let mut symbol_table = self.global_symbols.iter();
        while let Some(symbol) = symbol_table.next()? {
            if let Ok(symbol_data) = symbol.parse() {
                if let Some(current_symbol_name) = get_symbol_name(&symbol_data) {
                    if current_symbol_name == symbol_name {
                        return Ok(self
                            .reconstruct_symbol(
                                &type_finder,
                                &symbol,
                                primitives_flavor,
                                print_access_specifiers,
                            )
                            .unwrap_or_default());
                    }
                }
            }
        }

        // Modules' private symbols
        {
            let mut pdb = self.pdb.write().expect("lock shouldn't be poisoned");
            let mut modules = self.debug_information.modules()?;
            while let Some(module) = modules.next()? {
                if let Some(module_info) = pdb.module_info(&module)? {
                    let mut module_symbols = module_info.symbols()?;
                    while let Some(symbol) = module_symbols.next()? {
                        if let Ok(symbol_data) = symbol.parse() {
                            if let Some(current_symbol_name) = get_symbol_name(&symbol_data) {
                                if current_symbol_name == symbol_name {
                                    return Ok(self
                                        .reconstruct_symbol(
                                            &type_finder,
                                            &symbol,
                                            primitives_flavor,
                                            print_access_specifiers,
                                        )
                                        .unwrap_or_default());
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(ResymCoreError::SymbolNotFoundError(format!(
            "Symbol '{}' not found",
            symbol_name
        )))
    }

    pub fn reconstruct_all_symbols(
        &self,
        primitives_flavor: PrimitiveReconstructionFlavor,
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

        let mut reconstruction_output = String::new();

        // Global symbols
        let mut symbol_table = self.global_symbols.iter();
        while let Some(symbol) = symbol_table.next()? {
            if let Ok(symbol_data) = symbol.parse() {
                if get_symbol_name(&symbol_data).is_some() {
                    if let Some(reconstructed_symbol) = self.reconstruct_symbol(
                        &type_finder,
                        &symbol,
                        primitives_flavor,
                        print_access_specifiers,
                    ) {
                        writeln!(&mut reconstruction_output, "{}", reconstructed_symbol)?;
                    }
                }
            }
        }

        // Modules' private symbols
        {
            let mut pdb = self.pdb.write().expect("lock shouldn't be poisoned");
            let mut modules = self.debug_information.modules()?;
            while let Some(module) = modules.next()? {
                if let Some(module_info) = pdb.module_info(&module)? {
                    let mut module_symbols = module_info.symbols()?;
                    while let Some(symbol) = module_symbols.next()? {
                        if let Ok(symbol_data) = symbol.parse() {
                            if get_symbol_name(&symbol_data).is_some() {
                                if let Some(reconstructed_symbol) = self.reconstruct_symbol(
                                    &type_finder,
                                    &symbol,
                                    primitives_flavor,
                                    print_access_specifiers,
                                ) {
                                    writeln!(
                                        &mut reconstruction_output,
                                        "{}",
                                        reconstructed_symbol
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(reconstruction_output)
    }

    pub fn reconstruct_module_by_path(
        &self,
        module_path: &str,
        primitives_flavor: PrimitiveReconstructionFlavor,
        print_access_specifiers: bool,
    ) -> Result<String> {
        // Find index for module
        let mut modules = self.debug_information.modules()?;
        let module_index = modules.position(|module| Ok(module.module_name() == module_path))?;

        match module_index {
            None => Err(ResymCoreError::ModuleNotFoundError(format!(
                "Module '{}' not found",
                module_path
            ))),
            Some(module_index) => self.reconstruct_module_by_index(
                module_index,
                primitives_flavor,
                print_access_specifiers,
            ),
        }
    }

    pub fn reconstruct_module_by_index(
        &self,
        module_index: usize,
        primitives_flavor: PrimitiveReconstructionFlavor,
        print_access_specifiers: bool,
    ) -> Result<String> {
        let mut modules = self.debug_information.modules()?;
        let module = modules.nth(module_index)?.ok_or_else(|| {
            ResymCoreError::ModuleInfoNotFoundError(format!("Module #{} not found", module_index))
        })?;

        let module_info = self
            .pdb
            .write()
            .expect("lock shouldn't be poisoned")
            .module_info(&module)?
            .ok_or_else(|| {
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
            let reconstructed_symbol = self.reconstruct_symbol(
                &type_finder,
                &symbol,
                primitives_flavor,
                print_access_specifiers,
            );
            if let Some(reconstructed_symbol) = reconstructed_symbol {
                result += &reconstructed_symbol;
                result.push('\n');
            }

            Ok(())
        })?;

        Ok(result)
    }

    fn reconstruct_type_by_type_index_internal(
        &self,
        type_finder: &pdb::TypeFinder,
        type_index: TypeIndex,
        primitives_flavor: PrimitiveReconstructionFlavor,
        reconstruct_dependencies: bool,
        print_access_specifiers: bool,
        integers_as_hexadecimal: bool,
        ignore_std_types: bool,
    ) -> Result<ReconstructedType> {
        let fmt_configuration = DataFormatConfiguration {
            print_access_specifiers,
            integers_as_hexadecimal,
        };
        let mut type_data = pdb_types::Data::new(ignore_std_types);

        // If dependencies aren't needed, only process the given type index and return
        if !reconstruct_dependencies {
            let mut needed_types = pdb_types::NeededTypeSet::new();
            type_data.add(
                type_finder,
                &self.forwarder_to_complete_type,
                type_index.into(),
                &primitives_flavor,
                &mut needed_types,
            )?;

            let mut reconstruction_output = String::new();
            type_data.reconstruct(
                &fmt_configuration,
                &Default::default(),
                &mut reconstruction_output,
            )?;
            let needed_types: Vec<TypeIndex> = needed_types.into_iter().map(|e| e.0 .0).collect();
            let xrefs_from = self.type_list_from_type_indices(&needed_types);

            return Ok((reconstruction_output, xrefs_from));
        }

        let mut xrefs_from = vec![];
        // Add all the needed types iteratively until we're done
        let mut type_dependency_map: HashMap<TypeIndex, Vec<(TypeIndex, bool)>> = HashMap::new();
        {
            let dep_start = Instant::now();

            // Add the requested type first
            let mut types_to_process: VecDeque<TypeIndex> = VecDeque::from([type_index]);
            let mut processed_type_set = HashSet::new();
            // Keep processing new types until there's nothing to process
            while let Some(needed_type_index) = types_to_process.pop_front() {
                if processed_type_set.contains(&needed_type_index) {
                    // Already processed, continue
                    continue;
                }

                // Add the type
                let mut needed_types = pdb_types::NeededTypeSet::new();
                type_data.add(
                    type_finder,
                    &self.forwarder_to_complete_type,
                    needed_type_index.into(),
                    &primitives_flavor,
                    &mut needed_types,
                )?;
                // Initialize only once, the first time (i.e., for the requested type)
                if xrefs_from.is_empty() {
                    let needed_types: Vec<TypeIndex> =
                        needed_types.iter().map(|e| e.0 .0).collect();
                    xrefs_from = self.type_list_from_type_indices(&needed_types);
                }

                for (type_index, is_pointer) in &needed_types {
                    // Add forward declaration for types referenced by pointers
                    if *is_pointer {
                        type_data.add_as_forward_declaration(type_finder, *type_index)?;
                    }

                    // Update type dependency map
                    if let Some(type_dependency) = type_dependency_map.get_mut(&needed_type_index) {
                        type_dependency.push((type_index.0, *is_pointer));
                    } else {
                        type_dependency_map
                            .insert(needed_type_index, vec![(type_index.0, *is_pointer)]);
                    }
                }
                // Update the set of processed types
                processed_type_set.insert(needed_type_index);
                // Update the queue of type to process
                types_to_process.extend(needed_types.into_iter().map(|pair| pair.0 .0));
            }

            log::debug!(
                "Dependencies reconstruction took {} ms",
                dep_start.elapsed().as_millis()
            );
        }

        // Deduce type "depth" from the dependency map
        let type_depth_map = compute_type_depth_map(&type_dependency_map, &[type_index]);

        let mut reconstruction_output = String::new();
        type_data.reconstruct(
            &fmt_configuration,
            &type_depth_map,
            &mut reconstruction_output,
        )?;

        Ok((reconstruction_output, xrefs_from))
    }

    pub fn reconstruct_all_types(
        &self,
        primitives_flavor: PrimitiveReconstructionFlavor,
        print_access_specifiers: bool,
        integers_as_hexadecimal: bool,
        ignore_std_types: bool,
    ) -> Result<String> {
        let mut type_data = pdb_types::Data::new(ignore_std_types);
        let mut processed_types = Vec::new();
        let mut type_dependency_map: HashMap<TypeIndex, Vec<(TypeIndex, bool)>> = HashMap::new();
        {
            let mut type_finder = self.type_information.finder();
            // Populate our `TypeFinder`
            let mut type_iter = self.type_information.iter();
            while (type_iter.next()?).is_some() {
                type_finder.update(&type_iter);
            }

            // Add the requested types
            let mut type_iter = self.type_information.iter();
            while let Some(item) = type_iter.next()? {
                let mut needed_types = pdb_types::NeededTypeSet::new();
                // Note(ergelet): try to get the complete type's index here.
                // This avoids adding empty "forward reference" type index which
                // usually have lower type indices
                let complete_type_index = self
                    .forwarder_to_complete_type
                    .get(&item.index())
                    .map(|e| *e)
                    .unwrap_or_else(|| item.index());
                let result = type_data.add(
                    &type_finder,
                    &self.forwarder_to_complete_type,
                    complete_type_index,
                    &primitives_flavor,
                    &mut needed_types,
                );

                // Process result
                if let Err(err) = result {
                    // Handle error
                    match err {
                        ResymCoreError::PdbError(err) => {
                            // Ignore this kind of error since some particular PDB features might not be supported.
                            // This allows the recontruction to go through with the correctly reconstructed types.
                            log::warn!("Failed to reconstruct type with index {complete_type_index}: {err}")
                        }
                        _ => return Err(err),
                    }
                } else {
                    // Handle success
                    processed_types.push(complete_type_index.0);
                    for (type_index, is_pointer) in &needed_types {
                        // Add forward declaration for types referenced by pointers
                        if *is_pointer {
                            type_data.add_as_forward_declaration(&type_finder, *type_index)?;
                        }

                        // Update type dependency map
                        if let Some(type_dependency) =
                            type_dependency_map.get_mut(&complete_type_index.0)
                        {
                            type_dependency.push((type_index.0, *is_pointer));
                        } else {
                            type_dependency_map
                                .insert(complete_type_index.0, vec![(type_index.0, *is_pointer)]);
                        }
                    }
                }
            }
        }

        // Deduce type "depth" from the dependency map
        let type_depth_map = compute_type_depth_map(&type_dependency_map, &processed_types);

        let mut reconstruction_output = String::new();
        type_data.reconstruct(
            &DataFormatConfiguration {
                print_access_specifiers,
                integers_as_hexadecimal,
            },
            &type_depth_map,
            &mut reconstruction_output,
        )?;

        Ok(reconstruction_output)
    }

    pub fn get_xrefs_for_type(&self, type_index: TypeIndex) -> Result<TypeList> {
        // Generate xref cache if empty
        if self
            .xref_to_map
            .read()
            .expect("lock shouldn't be poisoned")
            .is_empty()
        {
            // Populate our `TypeFinder`
            let mut type_finder = self.type_information.finder();
            {
                let mut type_iter = self.type_information.iter();
                while (type_iter.next()?).is_some() {
                    type_finder.update(&type_iter);
                }
            }

            // Iterate through all types
            let xref_map: DashMap<TypeIndex, Vec<TypeIndex>> = DashMap::default();
            let mut type_iter = self.type_information.iter();
            while let Some(type_item) = type_iter.next()? {
                let current_type_index = type_item.index();
                // Reconstruct type and retrieve referenced types
                let mut type_data = pdb_types::Data::new(false);
                let mut needed_types = pdb_types::NeededTypeSet::new();
                let result = type_data.add(
                    &type_finder,
                    &self.forwarder_to_complete_type,
                    current_type_index,
                    &PrimitiveReconstructionFlavor::Raw,
                    &mut needed_types,
                );
                // Process result
                if let Err(err) = result {
                    // Handle error
                    match err {
                        ResymCoreError::PdbError(err) => {
                            // Ignore this kind of error since some particular PDB features might not be supported.
                            // This allows the recontruction to go through with the correctly reconstructed types.
                            log::warn!(
                                "Failed to reconstruct type with index {current_type_index}: {err}"
                            )
                        }
                        _ => return Err(err),
                    }
                }

                par_iter_if_available!(needed_types).for_each(|(t, _)| {
                    if let Some(mut xref_list) = xref_map.get_mut(&t.0) {
                        xref_list.push(current_type_index.0);
                    } else {
                        xref_map.insert(t.0, vec![current_type_index.0]);
                    }
                });
            }

            // Update cache
            if let Ok(mut xref_map_ref) = self.xref_to_map.write() {
                *xref_map_ref = xref_map;
            }
        }

        // Query xref cache
        if let Some(xref_list) = self
            .xref_to_map
            .read()
            .expect("lock shouldn't be poisoned")
            .get(&type_index)
        {
            // Convert the xref list into a proper Name+TypeIndex tuple list
            let xref_type_list = self.type_list_from_type_indices(&xref_list);

            Ok(xref_type_list)
        } else {
            // No xrefs found for the given type
            Ok(vec![])
        }
    }

    fn type_list_from_type_indices(&self, type_indices: &[TypeIndex]) -> TypeList {
        par_iter_if_available!(self.complete_type_list)
            .filter_map(|(type_name, type_index, _)| {
                if type_indices.contains(type_index) {
                    Some((type_name.clone(), *type_index))
                } else {
                    None
                }
            })
            .collect()
    }

    fn reconstruct_symbol(
        &self,
        type_finder: &pdb::ItemFinder<'_, pdb::TypeIndex>,
        symbol: &pdb::Symbol<'_>,
        primitives_flavor: PrimitiveReconstructionFlavor,
        print_access_specifiers: bool,
    ) -> Option<String> {
        let mut needed_types = pdb_types::NeededTypeSet::new();
        match symbol.parse().ok()? {
            pdb::SymbolData::UserDefinedType(udt) => {
                if let Ok(type_name) = type_name(
                    type_finder,
                    &self.forwarder_to_complete_type,
                    udt.type_index,
                    &primitives_flavor,
                    &mut needed_types,
                ) {
                    if type_name.0 == "..." {
                        // No type
                        Some(format!("char {}; // (missing type information)", udt.name))
                    } else {
                        Some(format!(
                            "using {} = {}{};",
                            udt.name, type_name.0, type_name.1
                        ))
                    }
                } else {
                    None
                }
            }

            // Functions and methods
            pdb::SymbolData::Procedure(procedure) => {
                let symbol_rva = symbol_rva(&procedure.offset, &self.address_map)
                    .map(|offset| format!("RVA=0x{:x} ", offset))
                    .unwrap_or_default();
                if let Ok(type_name) = type_name(
                    type_finder,
                    &self.forwarder_to_complete_type,
                    procedure.type_index,
                    &primitives_flavor,
                    &mut needed_types,
                ) {
                    let static_prefix = if procedure.global { "" } else { "static " };
                    if type_name.0 == "..." {
                        // No type
                        Some(format!(
                            "{}void {}(); // {}CodeSize=0x{:x} (missing type information)",
                            static_prefix, procedure.name, symbol_rva, procedure.len,
                        ))
                    } else {
                        Some(format!(
                            "{}{}{}{}; // {}CodeSize=0x{:x}",
                            static_prefix,
                            type_name.0,
                            procedure.name,
                            type_name.1,
                            symbol_rva,
                            procedure.len,
                        ))
                    }
                } else {
                    None
                }
            }

            // Global variables
            pdb::SymbolData::Data(data) => {
                let symbol_rva = symbol_rva(&data.offset, &self.address_map)
                    .map(|offset| format!("RVA=0x{:x} ", offset))
                    .unwrap_or_default();
                if let Ok(type_name) = type_name(
                    type_finder,
                    &self.forwarder_to_complete_type,
                    data.type_index,
                    &primitives_flavor,
                    &mut needed_types,
                ) {
                    let static_prefix = if data.global { "" } else { "static " };
                    if let Some(demangled_symbol) =
                        demangle_symbol_name(data.name.to_string(), print_access_specifiers)
                    {
                        Some(format!(
                            "{}{}; // {}",
                            static_prefix, demangled_symbol, symbol_rva,
                        ))
                    } else if type_name.0 == "..." {
                        // No type
                        Some(format!(
                            "{}char {}; // {}(missing type information)",
                            static_prefix, data.name, symbol_rva,
                        ))
                    } else {
                        Some(format!(
                            "{}{} {}{}; // {}",
                            static_prefix, type_name.0, data.name, type_name.1, symbol_rva,
                        ))
                    }
                } else {
                    None
                }
            }

            pdb::SymbolData::UsingNamespace(namespace) => {
                Some(format!("using namespace {};", namespace.name))
            }

            pdb::SymbolData::AnnotationReference(annotation) => {
                // TODO(ergrelet): update when support for annotations
                // (symbol kind 0x1019) has been implemented in `pdb`
                Some(format!("__annotation(); // {}", annotation.name))
            }

            // Public symbols
            pdb::SymbolData::Public(data) => {
                println!("{:x} {:x}", data.offset.offset, data.offset.section);
                let symbol_rva = symbol_rva(&data.offset, &self.address_map)
                    .map(|offset| format!("RVA=0x{:x} ", offset))
                    .unwrap_or_default();
                Some(
                    if let Some(demangled_symbol) =
                        demangle_symbol_name(data.name.to_string(), print_access_specifiers)
                    {
                        format!("{}; // {}", demangled_symbol, symbol_rva)
                    } else if data.function {
                        // Add parenthese to distinguish functions from global variables
                        format!(
                            "void {}(); // {}(no type information)",
                            data.name, symbol_rva,
                        )
                    } else {
                        format!("char {}; // {}(no type information)", data.name, symbol_rva,)
                    },
                )
            }

            // Exported symbols
            pdb::SymbolData::Export(data) => Some(
                if let Some(demangled_symbol) =
                    demangle_symbol_name(data.name.to_string(), print_access_specifiers)
                {
                    format!("{};", demangled_symbol)
                } else if data.flags.data {
                    format!("char {}; // Exported (no type information)", data.name)
                } else {
                    // Add parenthese to distinguish functions from exported variables
                    format!("void {}(); // Exported (no type information)", data.name)
                },
            ),

            _ => {
                // ignore everything else
                None
            }
        }
    }
}

fn compute_type_depth_map(
    type_dependency_map: &HashMap<TypeIndex, Vec<(TypeIndex, bool)>>,
    root_types: &[TypeIndex],
) -> BTreeMap<usize, Vec<pdb::TypeIndex>> {
    let depth_start = Instant::now();

    let mut type_depth_map: HashMap<TypeIndex, usize> =
        HashMap::from_iter(root_types.iter().map(|elem| (*elem, 0)));
    // Perform depth-first search to determine the "depth" of each type
    let mut types_to_visit: VecDeque<(usize, TypeIndex)> =
        VecDeque::from_iter(root_types.iter().map(|elem| (0, *elem)));
    while let Some((current_type_depth, current_type_index)) = types_to_visit.pop_back() {
        if let Some(type_dependencies) = type_dependency_map.get(&current_type_index) {
            for (child_type_index, child_is_pointer) in type_dependencies {
                // Visit child only if it's directly referenced, to avoid infinite loops
                if !child_is_pointer && *child_type_index != current_type_index {
                    let current_child_depth = current_type_depth + 1;
                    if let Some(child_type_depth) = type_depth_map.get_mut(child_type_index) {
                        *child_type_depth = std::cmp::max(*child_type_depth, current_child_depth);
                    } else {
                        type_depth_map.insert(*child_type_index, current_child_depth);
                    }
                    types_to_visit.push_back((current_child_depth, *child_type_index));
                }
            }
        }
    }

    // Invert type depth map
    let inverted_type_depth_map: BTreeMap<usize, Vec<pdb::TypeIndex>> = type_depth_map
        .into_iter()
        .fold(BTreeMap::new(), |mut acc, (type_index, type_depth)| {
            if let Some(type_indices) = acc.get_mut(&type_depth) {
                type_indices.push(type_index.into());
            } else {
                acc.insert(type_depth, vec![type_index.into()]);
            }

            acc
        });

    log::debug!(
        "Depth calculation took {} ms",
        depth_start.elapsed().as_millis()
    );

    inverted_type_depth_map
}

fn get_symbol_name(symbol_data: &pdb::SymbolData) -> Option<String> {
    const UNNAMED_CONSTANT_PREFIXES: [&str; 5] = ["`", "??_", "__@@_PchSym_", "__real@", "__xmm@"];
    const UNNAMED_CONSTANT_SUFFIXES: [&str; 1] = ["@@9@9"];

    match symbol_data {
        pdb::SymbolData::UserDefinedType(udt) => Some(udt.name.to_string().to_string()),

        // Functions and methods
        pdb::SymbolData::Procedure(procedure) => Some(procedure.name.to_string().to_string()),

        // Global variables
        pdb::SymbolData::Data(data) => Some(data.name.to_string().to_string()),

        // Public symbols
        pdb::SymbolData::Public(data) => Some(data.name.to_string().to_string()),

        // Exported symbols
        pdb::SymbolData::Export(data) => Some(data.name.to_string().to_string()),

        _ => {
            // ignore everything else
            None
        }
    }
    .filter(|name| {
        // Ignore unnamed constants
        for prefix in UNNAMED_CONSTANT_PREFIXES {
            if name.starts_with(prefix) {
                return false;
            }
        }
        for suffix in UNNAMED_CONSTANT_SUFFIXES {
            if name.ends_with(suffix) {
                return false;
            }
        }

        true
    })
}

fn get_symbol_type(symbol_data: &pdb::SymbolData) -> SymbolKind {
    match symbol_data {
        pdb::SymbolData::UserDefinedType(_) => SymbolKind::Type,

        // Functions and methods
        pdb::SymbolData::Procedure(_) => SymbolKind::Function,

        // Global variables
        pdb::SymbolData::Data(_) => SymbolKind::Variable,

        // Public symbols
        pdb::SymbolData::Public(data) => {
            if data.function {
                SymbolKind::Function
            } else {
                SymbolKind::Variable
            }
        }

        // Exported symbols
        pdb::SymbolData::Export(data) => {
            if data.flags.data {
                SymbolKind::Variable
            } else {
                SymbolKind::Function
            }
        }

        _ => SymbolKind::Unknown,
    }
}

fn symbol_rva(
    symbol_offset: &pdb::PdbInternalSectionOffset,
    translator: &AddressMap<'_>,
) -> Option<u32> {
    if symbol_offset.section == 0 {
        None
    }
    else {
        symbol_offset.to_internal_rva(translator)?.to_rva(translator).map(u32::from)
    }
}

fn demangle_symbol_name(
    symbol_name: impl AsRef<str>,
    print_access_specifiers: bool,
) -> Option<String> {
    const CXX_ACCESS_SPECIFIERS: [&str; 3] = ["public: ", "protected: ", "private: "];

    msvc_demangler::demangle(symbol_name.as_ref(), msvc_demangler::DemangleFlags::llvm())
        .map(|mut s| {
            if !print_access_specifiers {
                // Strip access specifiers
                CXX_ACCESS_SPECIFIERS.iter().for_each(|specifier| {
                    if let Some(stripped_s) = s.strip_prefix(specifier) {
                        s = stripped_s.to_string();
                    }
                });
            }

            s
        })
        .ok()
}

fn symbol_priority(symbol_data: &pdb::SymbolData) -> u16 {
    match symbol_data {
        // Functions and methods, user types, global variables
        pdb::SymbolData::Procedure(_)
        | pdb::SymbolData::UserDefinedType(_)
        | pdb::SymbolData::Data(_) => 0,
        // Public symbols
        pdb::SymbolData::Public(_) => 1,
        // Exported symbols
        pdb::SymbolData::Export(_) => 2,
        _ => 10,
    }
}
