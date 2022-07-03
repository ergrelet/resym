use anyhow::{anyhow, Result};

pub fn primitive_kind_as_str_portable(
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

        _ => Err(anyhow!(format!(
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
