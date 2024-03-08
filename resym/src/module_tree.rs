use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{anyhow, Result};

const MODULE_PATH_SEPARATOR: &str = "\\";

/// Tree of module paths, plus info at the leaves.
///
/// The tree contains a list of subtrees, and so on recursively.
pub struct ModuleTreeNode {
    /// Full path to the root of this tree
    pub path: ModulePath,
    /// Direct descendants of this (sub)tree
    pub children: HashMap<ModulePathPart, ModuleTreeNode>,

    /// Information on the module (only available for leaves)
    pub module_info: Option<ModuleInfo>,
}

impl ModuleTreeNode {
    /// Add a module to the tree
    pub fn add_module_by_path(
        &mut self,
        module_path: ModulePath,
        module_info: ModuleInfo,
    ) -> Result<()> {
        if !module_path.is_descendant_of(&self.path) {
            return Err(anyhow!("Module doesn't belong to the tree"));
        }

        // Direct child of ours
        if module_path.is_child_of(&self.path) {
            let result = match module_path.last() {
                None => return Err(anyhow!("Module path is empty")),
                Some(last_part) => {
                    // Direct child of the current node, add it to our children
                    self.add_child(last_part, Some(module_info));

                    Ok(())
                }
            };
            return result;
        }

        // Not a direct child of ours, pass it down to our children if we have any
        for child_node in self.children.values_mut() {
            if module_path.is_descendant_of(&child_node.path) {
                return child_node.add_module_by_path(module_path, module_info);
            }
        }

        // Not a direct child of ours, no children to pass it to.
        // We need to create the missing descendant(s)
        let start_index = self.path.len();
        let end_index = module_path.len();
        let mut current_node = self;
        for i in start_index..end_index {
            let new_child_part = &module_path.path.parts[i];
            let new_child_is_leaf = i == end_index - 1;
            let new_child_module_info = if new_child_is_leaf {
                Some(module_info)
            } else {
                None
            };
            current_node.add_child(new_child_part, new_child_module_info);
            // Note: if `get_mut` fails, it means there's a bug in our code
            current_node = current_node
                .children
                .get_mut(new_child_part)
                .expect("key should exist in map");
        }

        Ok(())
    }

    fn add_child(&mut self, path: &ModulePathPart, module_info: Option<ModuleInfo>) {
        self.children.insert(
            path.clone(),
            ModuleTreeNode {
                path: self.path.join(&ModulePath::new(vec![path.clone()])),
                children: Default::default(),
                module_info,
            },
        );
    }
}

impl Default for ModuleTreeNode {
    fn default() -> Self {
        Self {
            path: ModulePath::root(),
            children: Default::default(),
            module_info: Default::default(),
        }
    }
}

#[derive(Copy, Clone)]
pub struct ModuleInfo {
    pub pdb_index: usize,
}

type ModulePathPart = String;

#[derive(Clone, Eq, PartialEq)]
pub struct ModulePath {
    /// precomputed hash
    hash: ModulePathHash,

    // [`Arc`] used for cheap cloning, and to keep down the size of [`ModulePath`].
    // We mostly use the hash for lookups and comparisons anyway!
    path: Arc<ModulePathImpl>,
}

impl ModulePath {
    #[inline]
    pub fn root() -> Self {
        Self::from(ModulePathImpl::root())
    }

    #[inline]
    pub fn new(parts: Vec<ModulePathPart>) -> Self {
        Self::from(parts)
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &ModulePathPart> {
        self.path.iter()
    }

    pub fn last(&self) -> Option<&ModulePathPart> {
        self.path.last()
    }

    #[inline]
    pub fn as_slice(&self) -> &[ModulePathPart] {
        self.path.as_slice()
    }

    #[inline]
    pub fn is_root(&self) -> bool {
        self.path.is_root()
    }

    /// Is this a strict descendant of the given path.
    #[inline]
    pub fn is_descendant_of(&self, other: &ModulePath) -> bool {
        other.len() < self.len() && self.path.iter().zip(other.iter()).all(|(a, b)| a == b)
    }

    /// Is this a direct child of the other path.
    #[inline]
    pub fn is_child_of(&self, other: &ModulePath) -> bool {
        other.len() + 1 == self.len() && self.path.iter().zip(other.iter()).all(|(a, b)| a == b)
    }

    /// Number of parts
    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.path.len()
    }

    #[inline]
    pub fn hash(&self) -> ModulePathHash {
        self.hash
    }

    /// Precomputed 64-bit hash.
    #[inline]
    pub fn hash64(&self) -> u64 {
        self.hash.hash64()
    }

    /// Return [`None`] if root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        self.path.parent().map(Self::from)
    }

    pub fn join(&self, other: &Self) -> Self {
        self.iter().chain(other.iter()).cloned().collect()
    }
}

impl FromIterator<ModulePathPart> for ModulePath {
    fn from_iter<T: IntoIterator<Item = ModulePathPart>>(parts: T) -> Self {
        Self::new(parts.into_iter().collect())
    }
}

impl<'a> FromIterator<&'a ModulePathPart> for ModulePath {
    fn from_iter<T: IntoIterator<Item = &'a ModulePathPart>>(parts: T) -> Self {
        Self::new(parts.into_iter().cloned().collect())
    }
}

impl From<ModulePathImpl> for ModulePath {
    #[inline]
    fn from(path: ModulePathImpl) -> Self {
        Self {
            hash: ModulePathHash(Hash64::hash(&path)),
            path: Arc::new(path),
        }
    }
}

impl From<Vec<ModulePathPart>> for ModulePath {
    #[inline]
    fn from(path: Vec<ModulePathPart>) -> Self {
        Self {
            hash: ModulePathHash(Hash64::hash(&path)),
            path: Arc::new(ModulePathImpl { parts: path }),
        }
    }
}

impl From<&[ModulePathPart]> for ModulePath {
    #[inline]
    fn from(path: &[ModulePathPart]) -> Self {
        Self::from(path.to_vec())
    }
}

impl From<&str> for ModulePath {
    #[inline]
    fn from(path: &str) -> Self {
        Self::from(parse_module_path(path))
    }
}

impl From<String> for ModulePath {
    #[inline]
    fn from(path: String) -> Self {
        Self::from(path.as_str())
    }
}

impl ToString for ModulePath {
    fn to_string(&self) -> String {
        self.path.parts.join(MODULE_PATH_SEPARATOR)
    }
}

impl From<ModulePath> for String {
    #[inline]
    fn from(path: ModulePath) -> Self {
        path.to_string()
    }
}

fn parse_module_path(path: &str) -> Vec<ModulePathPart> {
    let path = Path::new(path);
    let parts = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::RootDir => None,
            std::path::Component::CurDir => Some(ModulePathPart::from(".")),
            std::path::Component::ParentDir => Some(ModulePathPart::from("..")),
            std::path::Component::Prefix(windows_prefix) => Some(ModulePathPart::from(
                windows_prefix.as_os_str().to_str().unwrap_or_default(),
            )),
            std::path::Component::Normal(part) => {
                Some(ModulePathPart::from(part.to_str().unwrap_or_default()))
            }
        })
        .collect();

    parts
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModulePathImpl {
    parts: Vec<ModulePathPart>,
}

impl ModulePathImpl {
    #[inline]
    pub fn root() -> Self {
        Self { parts: vec![] }
    }

    #[inline]
    pub fn new(parts: Vec<ModulePathPart>) -> Self {
        Self { parts }
    }

    #[inline]
    pub fn as_slice(&self) -> &[ModulePathPart] {
        self.parts.as_slice()
    }

    #[inline]
    pub fn is_root(&self) -> bool {
        self.parts.is_empty()
    }

    /// Number of components
    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &ModulePathPart> {
        self.parts.iter()
    }

    #[inline]
    pub fn last(&self) -> Option<&ModulePathPart> {
        self.parts.last()
    }

    #[inline]
    pub fn push(&mut self, comp: ModulePathPart) {
        self.parts.push(comp);
    }

    /// Return [`None`] if root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.parts.is_empty() {
            None
        } else {
            Some(Self::new(self.parts[..(self.parts.len() - 1)].to_vec()))
        }
    }
}

/// A 64 bit hash of [`ModulePath`] with very small risk of collision.
#[derive(Copy, Clone, Eq)]
pub struct ModulePathHash(Hash64);

impl ModulePathHash {
    /// Sometimes used as the hash of `None`.
    pub const NONE: ModulePathHash = ModulePathHash(Hash64::ZERO);

    /// From an existing u64. Use this only for data conversions.
    #[inline]
    pub fn from_u64(i: u64) -> Self {
        Self(Hash64::from_u64(i))
    }

    #[inline]
    pub fn hash64(&self) -> u64 {
        self.0.hash64()
    }

    #[inline]
    pub fn is_some(&self) -> bool {
        *self != Self::NONE
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}

impl std::hash::Hash for ModulePathHash {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::cmp::PartialEq for ModulePathHash {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

#[derive(Copy, Clone, Eq)]
pub struct Hash64(u64);

impl Hash64 {
    pub const ZERO: Hash64 = Hash64(0);

    pub fn hash(value: impl std::hash::Hash + Copy) -> Self {
        Self(hash(value))
    }

    /// From an existing u64. Use this only for data conversions.
    #[inline]
    pub fn from_u64(i: u64) -> Self {
        Self(i)
    }

    #[inline]
    pub fn hash64(&self) -> u64 {
        self.0
    }
}

impl std::hash::Hash for Hash64 {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

impl std::cmp::PartialEq for Hash64 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

pub const HASH_RANDOM_STATE: ahash::RandomState = ahash::RandomState::with_seeds(0, 1, 2, 3);

/// Hash the given value.
#[inline]
fn hash(value: impl std::hash::Hash) -> u64 {
    // Don't use ahash::AHasher::default() since it uses a random number for seeding the hasher on every application start.
    HASH_RANDOM_STATE.hash_one(&value)
}
