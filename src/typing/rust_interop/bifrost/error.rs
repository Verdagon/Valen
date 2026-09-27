
#[derive(Debug, Clone)]
pub enum ValenError {
  UnsupportedExportedName(String),
  DataCarryingCallbackStruct(String),
  UnsupportedCallbackType(String),
  TypingFailed(String),
}
impl ValenError {
  pub fn to_string(&self) -> String {
    match self {
      ValenError::TypingFailed(m) => m.clone(),
      ValenError::UnsupportedExportedName(m) => format!("UnsupportedExportedName({m})"),
      ValenError::DataCarryingCallbackStruct(m) => format!("DataCarryingCallbackStruct({m})"),
      ValenError::UnsupportedCallbackType(m) => format!("UnsupportedCallbackType({m})"),
    }
  }
}
