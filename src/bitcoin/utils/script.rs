use bitcoin::Script;

/// Different Bitcoin script types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptType {
    /// Pay to Witness Public Key Hash (P2WPKH)
    P2WPKH,
    /// Pay to Witness Script Hash (P2WSH)
    P2WSH,
    /// Pay to Public Key Hash (P2PKH)
    P2PKH,
    /// Pay to Script Hash (P2SH)
    P2SH,
    /// Other or unknown script type
    Other,
}

/// Identifies Bitcoin script type
pub fn get_script_type(script: &Script) -> ScriptType {
    let bytes = script.as_bytes();

    // P2WPKH pattern
    if bytes.len() == 22 && bytes[0] == 0x00 && bytes[1] == 0x14 {
        return ScriptType::P2WPKH;
    }

    // P2WSH pattern
    if bytes.len() == 34 && bytes[0] == 0x00 && bytes[1] == 0x20 {
        return ScriptType::P2WSH;
    }

    // P2PKH pattern
    if bytes.len() == 25
        && bytes[0] == 0x76
        && bytes[1] == 0xa9
        && bytes[2] == 0x14
        && bytes[23] == 0x88
        && bytes[24] == 0xac
    {
        return ScriptType::P2PKH;
    }

    // P2SH pattern
    if bytes.len() == 23 && bytes[0] == 0xa9 && bytes[1] == 0x14 && bytes[22] == 0x87 {
        return ScriptType::P2SH;
    }

    // Default to Other
    ScriptType::Other
}
