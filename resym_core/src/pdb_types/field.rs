use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<'p> {
    pub type_left: String,
    pub type_right: String,
    pub name: pdb::RawString<'p>,
    /// Offset of the field in bytes within the current struct/union.
    pub offset: u64,
    /// Size of the field in bytes.
    pub size: usize,
    /// Offset and size of the field in bits, within the current type's size (0 means LSB).
    /// Present only for bitfield members.
    pub bitfield_info: Option<(u8, u8)>,
    pub access: FieldAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticField<'p> {
    pub type_left: String,
    pub type_right: String,
    pub name: pdb::RawString<'p>,
    pub access: FieldAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldAccess {
    None,
    Private,
    Protected,
    Public,
}
impl FieldAccess {
    pub fn from_field_attribute(value: u8) -> Self {
        match value {
            0 => FieldAccess::None,
            1 => FieldAccess::Private,
            2 => FieldAccess::Protected,
            3 => FieldAccess::Public,
            _ => unreachable!("Major PDB format update?"),
        }
    }
}
impl fmt::Display for FieldAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FieldAccess::None => "",
                FieldAccess::Private => "private: ",
                FieldAccess::Protected => "protected: ",
                FieldAccess::Public => "public: ",
            }
        )
    }
}
