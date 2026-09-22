use crate::postparsing::itemplatatype::{ITemplataType, KindTemplataType};
use crate::postparsing::names::IRuneS;
use crate::postparsing::patterns::patterns::AtomSP;

pub fn get_rune_types_from_pattern<'s>(
  pattern: &'s AtomSP<'s>,
) -> Vec<(IRuneS<'s>, ITemplataType<'s>)> {
  let mut runes_from_destructures: Vec<(IRuneS<'s>, ITemplataType<'s>)> = Vec::new();
  if let Some(destructure) = pattern.destructure {
    for sub_pattern in destructure {
      runes_from_destructures.extend(get_rune_types_from_pattern(sub_pattern));
    }
  }
  if let Some(kind_rune) = pattern.kind_rune {
    runes_from_destructures
      .push((kind_rune.rune, ITemplataType::KindTemplataType(KindTemplataType {})));
  }
  let mut result: Vec<(IRuneS<'s>, ITemplataType<'s>)> = Vec::new();
  for item in runes_from_destructures {
    if !result.contains(&item) {
      result.push(item);
    }
  }
  result
}
