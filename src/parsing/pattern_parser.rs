use crate::keywords::Keywords;
use crate::lexing::ast::*;
use crate::lexing::errors::ParseError;
use crate::parse_arena::ParseArena;
use crate::parsing::ast::*;
use crate::parsing::expression_parser::ScrambleIterator;
use crate::parsing::templex_parser::TemplexParser;

type ParseResult<T> = Result<T, ParseError>;

pub struct PatternParser<'p, 'ctx> {
  parse_arena: &'ctx ParseArena<'p>,
  keywords: &'ctx Keywords<'p>,
}

impl<'p, 'ctx> PatternParser<'p, 'ctx>
where
  'p: 'ctx,
{
  pub fn new(parse_arena: &'ctx ParseArena<'p>, keywords: &'ctx Keywords<'p>) -> Self {
    PatternParser { parse_arena, keywords }
  }

  pub fn parse_parameter(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
    templex_parser: &TemplexParser<'p, 'ctx>,
    index: usize,
    is_in_citizen: bool,
    is_in_function: bool,
    is_in_lambda: bool,
  ) -> ParseResult<ParameterP<'p>> {
    let pattern_begin = iter.get_pos();
    let pattern_range = iter.range();

    if !iter.has_next() {
      return Err(ParseError::EmptyPattern(pattern_begin));
    }

    let maybe_virtual = match iter.peek_cloned() {
      None => return Err(ParseError::EmptyParameter(pattern_range.begin())),
      Some(INodeLEEnum::Word(WordLE { range, str })) if str == self.keywords.r#virtual => {
        iter.advance();
        Some(AbstractP { range })
      }
      Some(_) => None,
    };

    let maybe_self_borrow = match iter.peek_n(2).as_slice() {
      [] => return Err(ParseError::EmptyParameter(pattern_range.begin())),
      [None] => return Err(ParseError::EmptyParameter(pattern_range.begin())),
      [None, None] => return Err(ParseError::EmptyParameter(pattern_range.begin())),
      [Some(INodeLEEnum::Symbol(SymbolLE(range1, '&'))), Some(INodeLEEnum::Word(WordLE { range: range2, str }))]
        if *str == self.keywords.self_ =>
      {
        let begin = range1.begin();
        let end = range2.end();
        iter.advance();
        iter.advance();
        Some(RangeL::new(begin, end))
      }
      _ => None,
    };

    if let Some(_) = maybe_self_borrow {
      return Ok(ParameterP {
        range: pattern_range,
        virtuality: maybe_virtual,
        self_borrow: maybe_self_borrow,
        pattern: None,
      });
    }

    let maybe_name = match iter.peek2_cloned() {
      (Some(INodeLEEnum::Squared(_)), _) => {
        None
      }
      (Some(INodeLEEnum::Word(_)), Some(INodeLEEnum::Squared(_))) => {
        None
      }
      (Some(INodeLEEnum::Word(w)), _) => {
        let word = w.clone();
        iter.advance();
        Some(word)
      }
      _ => return Err(ParseError::BadLocalName(iter.get_pos())),
    };

    let pattern = self.parse_pattern(
      iter,
      templex_parser,
      pattern_begin,
      index,
      is_in_citizen,
      is_in_function,
      is_in_lambda,
      /*is_parameter=*/ true,
      maybe_name,
    )?;

    Ok(ParameterP {
      range: pattern_range,
      virtuality: maybe_virtual,
      self_borrow: maybe_self_borrow,
      pattern: Some(pattern),
    })
  }

  pub fn parse_pattern(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
    templex_parser: &TemplexParser<'p, 'ctx>,
    pattern_begin: i32,
    index: usize,
    is_in_citizen: bool,
    is_in_function: bool,
    is_in_lambda: bool,
    is_parameter: bool,
    maybe_name_from_parameter: Option<WordLE<'p>>,
  ) -> ParseResult<PatternPP<'p>> {
    if !iter.has_next() {
      if maybe_name_from_parameter.is_none() {
        return Err(ParseError::EmptyPattern(pattern_begin));
      }
    }

    let is_constructing = match iter.peek2_cloned() {
      (
        Some(INodeLEEnum::Word(WordLE { str: self_str, .. })),
        Some(INodeLEEnum::Symbol(SymbolLE(_, '.'))),
      ) if self_str == self.keywords.self_ => {
        iter.advance();
        iter.advance();
        true
      }
      _ => false,
    };

    let maybe_mutate = iter.try_skip_word(self.keywords.set);
    if maybe_mutate.is_some() && !iter.has_next() {
      return Err(ParseError::CantUseThatLocalName {
        pos: iter.get_pos(),
        name: "set".to_string(),
      });
    }

    let maybe_destination_local = match maybe_name_from_parameter {
      Some(WordLE { range, str }) => {
        if str == self.keywords.underscore {
          Some(DestinationLocalP::<'p> {
            decl: INameDeclarationP::IgnoredLocalNameDeclaration(range),
            mutate: None,
          })
        } else {
          Some(DestinationLocalP::<'p> {
            decl: INameDeclarationP::LocalNameDeclaration(NameP(range, str)),
            mutate: None,
          })
        }
      }
      None => {
        let name_is_next = match iter.peek2_cloned() {
          (None, None) => {
            panic!("Impossible: peek2 should not return (None, None) when has_next is true")
          }
          (None, Some(_)) => panic!("Impossible: peek2 should not return (None, Some(_))"),
          (Some(_), None) => true,
          (Some(first), Some(second)) => {
            first.range().end() < second.range().begin()
          }
        };

        if name_is_next {
          match iter.peek_cloned() {
            Some(INodeLEEnum::Word(WordLE { range, str })) => {
              iter.advance();
              if str == self.keywords.underscore {
                Some(DestinationLocalP {
                  decl: INameDeclarationP::IgnoredLocalNameDeclaration(range),
                  mutate: maybe_mutate,
                })
              } else {
                if is_constructing {
                  Some(DestinationLocalP {
                    decl: INameDeclarationP::ConstructingMemberNameDeclaration(NameP(range, str)),
                    mutate: maybe_mutate,
                  })
                } else {
                  Some(DestinationLocalP {
                    decl: INameDeclarationP::LocalNameDeclaration(NameP(range, str)),
                    mutate: maybe_mutate,
                  })
                }
              }
            }
            Some(INodeLEEnum::Squared(_)) => None,
            _ => return Err(ParseError::BadLocalName(iter.get_pos())),
          }
        } else {
          None
        }
      }
    };

    match iter.peek_cloned() {
      Some(INodeLEEnum::Word(WordLE { str, .. })) if str == self.keywords.r#in => {
      }
      _ => {}
    }

    let next_is_type = match iter.peek2_cloned() {
      (None, None) => false,
      (None, Some(_)) => panic!("Impossible: peek2 should not return (None, Some(_))"),
      (Some(INodeLEEnum::Squared(_)), maybe_after) => {
        maybe_after.is_some()
      }
      (Some(_), _) => {
        true
      }
    };

    let maybe_type: Option<ITemplexPT<'p>> = if next_is_type {
      Some(templex_parser.parse_templex(iter)?)
    } else {
      if is_in_lambda {
        None
      } else if is_in_citizen {
        None
      } else if is_in_function {
        return Err(ParseError::LightFunctionMustHaveParamTypes {
          pos: pattern_begin,
          param_index: index as i32,
        });
      } else {
        // Allow it, just a regular pattern
        None
      }
    };

    if is_parameter {
      let _ = iter.try_skip_word(self.keywords.r#mut);
    }

    let maybe_destructure = match iter.peek_cloned() {
      Some(INodeLEEnum::Squared(SquaredLE {
        range: destructure_range,
        contents: destructure_elements,
      })) => {
        let destructure_elements = destructure_elements.clone();
        iter.advance();

        let destructure_iter = ScrambleIterator::new(&destructure_elements);
        let element_iters = destructure_iter.split_on_symbol(',', false);

        let mut patterns = Vec::new();
        for (index, mut element_iter) in element_iters.into_iter().enumerate() {
          let pos = element_iter.get_pos();
          let pattern = self.parse_pattern(
            &mut element_iter,
            templex_parser,
            pos,
            index,
            false,
            false,
            false,
            /*is_parameter=*/ false,
            None,
          )?;
          patterns.push(pattern);
        }

        Some(DestructureP {
          range: destructure_range,
          patterns: self.parse_arena.alloc_slice_from_vec(patterns),
        })
      }
      Some(other) => return Err(ParseError::BadThingAfterTypeInPattern(other.range().begin())),
      None => None,
    };

    Ok(PatternPP::<'p> {
      range: RangeL::new(pattern_begin, iter.get_prev_end_pos()),
      destination: maybe_destination_local,
      templex: maybe_type,
      destructure: maybe_destructure,
    })
  }
}
