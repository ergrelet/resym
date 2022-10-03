use std::str::FromStr;

use crate::error::{Result, ResymCoreError};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrimitiveReconstructionFlavor {
    Portable,
    Microsoft,
    Raw,
}

impl FromStr for PrimitiveReconstructionFlavor {
    type Err = ResymCoreError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "portable" => Ok(PrimitiveReconstructionFlavor::Portable),
            "ms" | "msft" | "microsoft" => Ok(PrimitiveReconstructionFlavor::Microsoft),
            "raw" => Ok(PrimitiveReconstructionFlavor::Raw),
            _ => Err(ResymCoreError::ParsePrimitiveFlavorError(s.to_owned())),
        }
    }
}

pub fn include_headers_for_flavor(flavor: PrimitiveReconstructionFlavor) -> String {
    match flavor {
        PrimitiveReconstructionFlavor::Portable => "#include <cstdint>\n",
        PrimitiveReconstructionFlavor::Microsoft => "#include <Windows.h>\n",
        PrimitiveReconstructionFlavor::Raw => "",
    }
    .to_string()
}

pub fn primitive_kind_as_str(
    flavor: &PrimitiveReconstructionFlavor,
    primitive_kind: pdb::PrimitiveKind,
    indirection: bool,
) -> Result<String> {
    match flavor {
        PrimitiveReconstructionFlavor::Portable => {
            primitive_kind_as_str_portable(primitive_kind, indirection)
        }
        PrimitiveReconstructionFlavor::Microsoft => {
            primitive_kind_as_str_microsoft(primitive_kind, indirection)
        }
        PrimitiveReconstructionFlavor::Raw => {
            primitive_kind_as_str_raw(primitive_kind, indirection)
        }
    }
}

fn primitive_kind_as_str_portable(
    primitive_kind: pdb::PrimitiveKind,
    indirection: bool,
) -> Result<String> {
    let str_representation = match primitive_kind {
        pdb::PrimitiveKind::Void => Ok("void"),
        pdb::PrimitiveKind::Char | pdb::PrimitiveKind::RChar => Ok("char"),
        pdb::PrimitiveKind::UChar => Ok("unsigned char"),
        pdb::PrimitiveKind::WChar => Ok("wchar_t"),
        pdb::PrimitiveKind::RChar16 => Ok("char16_t"),
        pdb::PrimitiveKind::RChar32 => Ok("char32_t"),

        pdb::PrimitiveKind::I8 => Ok("int8_t"),
        pdb::PrimitiveKind::U8 => Ok("uint8_t"),
        pdb::PrimitiveKind::I16 | pdb::PrimitiveKind::Short => Ok("int16_t"),
        pdb::PrimitiveKind::U16 | pdb::PrimitiveKind::UShort => Ok("uint16_t"),
        pdb::PrimitiveKind::I32 | pdb::PrimitiveKind::Long => Ok("int32_t"),
        pdb::PrimitiveKind::U32 | pdb::PrimitiveKind::ULong => Ok("uint32_t"),
        pdb::PrimitiveKind::I64 | pdb::PrimitiveKind::Quad => Ok("int64_t"),
        pdb::PrimitiveKind::U64 | pdb::PrimitiveKind::UQuad => Ok("uint64_t"),

        pdb::PrimitiveKind::F32 => Ok("float"),
        pdb::PrimitiveKind::F64 => Ok("double"),

        pdb::PrimitiveKind::Bool8 => Ok("bool"),
        pdb::PrimitiveKind::Bool32 => Ok("int32_t"),

        // Microsoft-specific, usually implemented as "long"
        pdb::PrimitiveKind::HRESULT => Ok("int32_t"),

        // TODO: Seems valid for C++ method parameters. Are there other
        // cases of legitimate "NoType" occurences?
        pdb::PrimitiveKind::NoType => Ok("..."),

        _ => Err(ResymCoreError::NotImplementedError(format!(
            "/* FIXME: Unhandled primitive kind: '{:?}' */ void",
            primitive_kind
        ))),
    };

    let mut string_representation = str_representation?.to_string();
    if indirection {
        string_representation.push('*');
    }

    Ok(string_representation)
}

fn primitive_kind_as_str_microsoft(
    primitive_kind: pdb::PrimitiveKind,
    indirection: bool,
) -> Result<String> {
    let str_representation = match primitive_kind {
        pdb::PrimitiveKind::Void => Ok(if indirection { "PVOID" } else { "VOID" }),
        pdb::PrimitiveKind::Char | pdb::PrimitiveKind::RChar | pdb::PrimitiveKind::I8 => {
            Ok(if indirection { "PCHAR" } else { "CHAR" })
        }
        pdb::PrimitiveKind::UChar | pdb::PrimitiveKind::U8 => {
            Ok(if indirection { "PUCHAR" } else { "UCHAR" })
        }
        pdb::PrimitiveKind::WChar => Ok(if indirection { "PWCHAR" } else { "WCHAR" }),
        pdb::PrimitiveKind::RChar16 => Ok(if indirection { "char16_t*" } else { "char16_t" }),
        pdb::PrimitiveKind::RChar32 => Ok(if indirection { "char32_t*" } else { "char32_t" }),

        pdb::PrimitiveKind::I16 | pdb::PrimitiveKind::Short => {
            Ok(if indirection { "PSHORT" } else { "SHORT" })
        }
        pdb::PrimitiveKind::U16 | pdb::PrimitiveKind::UShort => {
            Ok(if indirection { "PUSHORT" } else { "USHORT" })
        }
        pdb::PrimitiveKind::I32 | pdb::PrimitiveKind::Long => {
            Ok(if indirection { "PLONG" } else { "LONG" })
        }
        pdb::PrimitiveKind::U32 | pdb::PrimitiveKind::ULong => {
            Ok(if indirection { "PULONG" } else { "ULONG" })
        }
        pdb::PrimitiveKind::I64 | pdb::PrimitiveKind::Quad => {
            Ok(if indirection { "PLONGLONG" } else { "LONGLONG" })
        }
        pdb::PrimitiveKind::U64 | pdb::PrimitiveKind::UQuad => Ok(if indirection {
            "PULONGLONG"
        } else {
            "ULONGLONG"
        }),

        pdb::PrimitiveKind::F32 => Ok(if indirection { "PFLOAT" } else { "FLOAT" }),
        pdb::PrimitiveKind::F64 => Ok(if indirection { "DOUBLE*" } else { "DOUBLE" }),

        pdb::PrimitiveKind::Bool8 => Ok(if indirection { "PBOOLEAN" } else { "BOOLEAN" }),
        pdb::PrimitiveKind::Bool32 => Ok(if indirection { "PBOOL" } else { "BOOL" }),

        // Microsoft-specific
        pdb::PrimitiveKind::HRESULT => Ok(if indirection { "HRESULT*" } else { "HRESULT" }),

        // TODO: Seems valid for C++ method parameters. Are there other
        // cases of legitimate "NoType" occurences?
        pdb::PrimitiveKind::NoType => Ok("..."),

        _ => Err(ResymCoreError::NotImplementedError(format!(
            "/* FIXME: Unhandled primitive kind: '{:?}' */ void",
            primitive_kind
        ))),
    };

    Ok(str_representation?.to_string())
}

fn primitive_kind_as_str_raw(
    primitive_kind: pdb::PrimitiveKind,
    indirection: bool,
) -> Result<String> {
    let str_representation = match primitive_kind {
        pdb::PrimitiveKind::Void => Ok("void"),
        pdb::PrimitiveKind::I8 | pdb::PrimitiveKind::Char | pdb::PrimitiveKind::RChar => Ok("char"),
        pdb::PrimitiveKind::U8 | pdb::PrimitiveKind::UChar => Ok("unsigned char"),
        pdb::PrimitiveKind::WChar => Ok("wchar_t"),
        pdb::PrimitiveKind::RChar16 => Ok("char16_t"),
        pdb::PrimitiveKind::RChar32 => Ok("char32_t"),

        pdb::PrimitiveKind::I16 | pdb::PrimitiveKind::Short => Ok("short"),
        pdb::PrimitiveKind::U16 | pdb::PrimitiveKind::UShort => Ok("unsigned short"),
        pdb::PrimitiveKind::I32 | pdb::PrimitiveKind::Long => Ok("long"),
        pdb::PrimitiveKind::U32 | pdb::PrimitiveKind::ULong => Ok("unsigned long"),
        pdb::PrimitiveKind::I64 | pdb::PrimitiveKind::Quad => Ok("__int64"),
        pdb::PrimitiveKind::U64 | pdb::PrimitiveKind::UQuad => Ok("unsigned __int64"),

        pdb::PrimitiveKind::F32 => Ok("float"),
        pdb::PrimitiveKind::F64 => Ok("double"),

        pdb::PrimitiveKind::Bool8 => Ok("bool"),
        pdb::PrimitiveKind::Bool32 => Ok("long"),

        // Microsoft-specific, usually implemented as "long"
        pdb::PrimitiveKind::HRESULT => Ok("long"),

        // TODO: Seems valid for C++ method parameters. Are there other
        // cases of legitimate "NoType" occurences?
        pdb::PrimitiveKind::NoType => Ok("..."),

        _ => Err(ResymCoreError::NotImplementedError(format!(
            "/* FIXME: Unhandled primitive kind: '{:?}' */ void",
            primitive_kind
        ))),
    };

    let mut string_representation = str_representation?.to_string();
    if indirection {
        string_representation.push('*');
    }

    Ok(string_representation)
}
