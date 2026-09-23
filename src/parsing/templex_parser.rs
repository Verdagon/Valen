use crate::keywords::Keywords;
use crate::lexing::ast::*;
use crate::lexing::errors::ParseError;
use crate::parse_arena::ParseArena;
use crate::parsing::ast::*;
use crate::parsing::expression_parser::ScrambleIterator;
use crate::parsing::parse_utils::{parse_region, try_skip_past_equals_while};

type ParseResult<T> = Result<T, ParseError>;

pub struct TemplexParser<'p, 'ctx> {
  parse_arena: &'ctx ParseArena<'p>,
  keywords: &'ctx Keywords<'p>,
}

impl<'p, 'ctx> TemplexParser<'p, 'ctx>
where
  'p: 'ctx,
{
  pub fn new(parse_arena: &'ctx ParseArena<'p>, keywords: &'ctx Keywords<'p>) -> Self {
    TemplexParser { parse_arena, keywords }
  }

  pub fn parse_array(
    &self,
    original_iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<ITemplexPT<'p>>> {
    let begin = original_iter.get_pos();

    let mut tentative_iter = original_iter.clone();

    let squared_contents;
    let size_scramble_iter_l = match tentative_iter.peek_cloned() {
      Some(INodeLEEnum::Squared(squared)) => {
        squared_contents = squared.contents.clone();
        tentative_iter.advance();
        ScrambleIterator::new(&squared_contents)
      }
      _ => return Ok(None),
    };

    if size_scramble_iter_l.has_next() {
      return Ok(None);
    }

    original_iter.skip_to(&tentative_iter);
    let iter = original_iter;

    let element_type = self.parse_templex(iter)?;

    let result = ITemplexPT::RuntimeSizedArray(RuntimeSizedArrayPT {
      range: RangeL::new(begin, iter.get_prev_end_pos()),
      element: &*self.parse_arena.alloc(element_type),
    });

    Ok(Some(result))
  }

  pub fn parse_function_name(&self, iter: &mut ScrambleIterator<'p, '_>) -> Option<NameP<'p>> {
    match iter.peek_cloned() {
      Some(INodeLEEnum::Word(word)) => {
        let range = word.range;
        let str = word.str;
        iter.advance();
        Some(NameP(range, str))
      }
      Some(INodeLEEnum::Symbol(_)) => {
        let begin = iter.get_pos();
        match iter.peek3_cloned() {
          (
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
          ) => {
            iter.advance();
            iter.advance();
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.triple_equals))
          }
          (
            Some(INodeLEEnum::Symbol(SymbolLE(_, '<'))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '>'))),
          ) => {
            iter.advance();
            iter.advance();
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.spaceship))
          }
          (
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            _,
          ) => {
            iter.advance();
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.double_equals))
          }
          (
            Some(INodeLEEnum::Symbol(SymbolLE(_, '!'))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            _,
          ) => {
            iter.advance();
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.not_equals))
          }
          (
            Some(INodeLEEnum::Symbol(SymbolLE(_, '<'))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            _,
          ) => {
            iter.advance();
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.less_equals))
          }
          (
            Some(INodeLEEnum::Symbol(SymbolLE(_, '>'))),
            Some(INodeLEEnum::Symbol(SymbolLE(_, '='))),
            _,
          ) => {
            iter.advance();
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.greater_equals))
          }
          (Some(INodeLEEnum::Symbol(SymbolLE(_, '<'))), _, _) => {
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.less))
          }
          (Some(INodeLEEnum::Symbol(SymbolLE(_, '>'))), _, _) => {
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.greater))
          }
          (Some(INodeLEEnum::Symbol(SymbolLE(_, '+'))), _, _) => {
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.plus))
          }
          (Some(INodeLEEnum::Symbol(SymbolLE(_, '-'))), _, _) => {
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.minus))
          }
          (Some(INodeLEEnum::Symbol(SymbolLE(_, '*'))), _, _) => {
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.asterisk))
          }
          (Some(INodeLEEnum::Symbol(SymbolLE(_, '/'))), _, _) => {
            iter.advance();
            Some(NameP(RangeL::new(begin, iter.get_prev_end_pos()), self.keywords.slash))
          }
          _ => None,
        }
      }
      Some(INodeLEEnum::Parend(ParendLE { range, .. })) => {
        Some(NameP(RangeL::new(range.begin(), range.begin()), self.keywords.underscores_call))
      }
      _ => None,
    }
  }

  pub fn parse_prototype(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<ITemplexPT<'p>>> {
    let begin = iter.get_pos();

    if iter.try_skip_word(self.keywords.func).is_none() {
      return Ok(None);
    }

    let name = match self.parse_function_name(iter) {
      Some(n) => n,
      None => return Err(ParseError::BadPrototypeName(iter.get_pos())),
    };

    let args_begin = iter.get_pos();
    let args = match iter.peek_cloned() {
      Some(INodeLEEnum::Parend(ParendLE { contents, .. })) => {
        let contents = contents.clone();
        iter.advance();
        let mut elements: Vec<&'p ITemplexPT<'p>> = Vec::new();
        for mut element_iter in ScrambleIterator::new(&contents).split_on_symbol(',', false) {
          elements.push(&*self.parse_arena.alloc(self.parse_templex(&mut element_iter)?));
          let _ = element_iter.try_skip_word(self.keywords.r#mut);
          if element_iter.has_next() {
            return Err(ParseError::BadPrototypeParams(element_iter.get_pos()));
          }
        }
        self.parse_arena.alloc_slice_from_vec(elements)
      }
      _ => return Err(ParseError::BadPrototypeParams(iter.get_pos())),
    };
    let args_end = iter.get_prev_end_pos();

    let return_type = self.parse_templex(iter)?;

    let result = ITemplexPT::Func(FuncPT {
      range: RangeL::new(begin, iter.get_prev_end_pos()),
      name,
      params_range: RangeL::new(args_begin, args_end),
      parameters: args,
      return_type: &*self.parse_arena.alloc(return_type),
    });

    Ok(Some(result))
  }

  pub fn parse_ref_prefix(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<ITemplexPT<'p>>> {
    let begin = iter.get_pos();

    if iter.try_skip_word(self.keywords.weak).is_some() {
      let inner = self.parse_templex_atom_and_call_and_prefixes(iter)?;
      return Ok(Some(ITemplexPT::WeakRef(WeakRefPT {
        range: RangeL::new(begin, iter.get_prev_end_pos()),
        inner: &*self.parse_arena.alloc(inner),
      })));
    }

    if iter.try_skip_word(self.keywords.own).is_some() {
      let inner = self.parse_templex_atom_and_call_and_prefixes(iter)?;
      return Ok(Some(ITemplexPT::OwnRef(OwnRefPT {
        range: RangeL::new(begin, iter.get_prev_end_pos()),
        inner: &*self.parse_arena.alloc(inner),
      })));
    }

    if iter.try_skip_word(self.keywords.held).is_some() {
      let inner = self.parse_templex_atom_and_call_and_prefixes(iter)?;
      return Ok(Some(ITemplexPT::BorrowRef(BorrowRefPT {
        range: RangeL::new(begin, iter.get_prev_end_pos()),
        inner: &*self.parse_arena.alloc(inner),
        region: RegionP::Held,
      })));
    }

    if iter.try_skip_word(self.keywords.r#dyn).is_some() {
      let inner = self.parse_templex_atom_and_call_and_prefixes(iter)?;
      return Ok(Some(ITemplexPT::DynInterface(DynInterfacePT {
        range: RangeL::new(begin, iter.get_prev_end_pos()),
        inner: &*self.parse_arena.alloc(inner),
      })));
    }

    if iter.try_skip_symbol('&') {
      let inner = self.parse_templex_atom_and_call_and_prefixes(iter)?;
      let region = self.parse_trailing_group_clause(iter)?;
      return Ok(Some(ITemplexPT::BorrowRef(BorrowRefPT {
        range: RangeL::new(begin, iter.get_prev_end_pos()),
        inner: &*self.parse_arena.alloc(inner),
        region,
      })));
    }

    Ok(None)
  }

  fn parse_trailing_group_clause(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<RegionP<'p>> {
    if iter.try_skip_word(self.keywords.r#in).is_none() {
      return Ok(RegionP::Unspecified);
    }
    Ok(RegionP::Group(self.parse_group(iter)?))
  }

  pub fn parse_group(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<&'p GroupP<'p>> {
    let mut group = match iter.peek_cloned() {
      Some(INodeLEEnum::Word(WordLE { range, str })) => {
        iter.advance();
        &*self.parse_arena.alloc(GroupP::Name(NameP(range, str)))
      }
      _ => return Err(ParseError::BadTypeExpression(iter.get_pos())),
    };
    loop {
      if iter.try_skip_symbols(&['.', '.', '.']) {
        group = &*self.parse_arena.alloc(GroupP::Ellipsis { base: group });
        break;
      }
      match iter.peek_cloned() {
        Some(INodeLEEnum::Symbol(SymbolLE(_, '.'))) => {
          iter.advance();
          match iter.peek_cloned() {
            Some(INodeLEEnum::Word(WordLE { range, str })) => {
              iter.advance();
              group =
                &*self.parse_arena.alloc(GroupP::Member { base: group, member: NameP(range, str) });
            }
            _ => return Err(ParseError::BadTypeExpression(iter.get_pos())),
          }
        }
        Some(INodeLEEnum::Squared(squared)) => {
          let contents = squared.contents.clone();
          if ScrambleIterator::new(&contents).has_next() {
            break;
          }
          iter.advance();
          group = &*self.parse_arena.alloc(GroupP::Elements { base: group });
        }
        _ => break,
      }
    }
    Ok(group)
  }

  pub fn parse_ending_region(
    &self,
    original_iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<RegionRunePT<'p>>> {
    let mut tentative_iter = original_iter.clone();

    let region = match parse_region(&mut tentative_iter)? {
      None => return Ok(None),
      Some(region_rune) => region_rune,
    };

    if tentative_iter.has_next() {
      return Ok(None);
    }

    original_iter.skip_to(&tentative_iter);

    Ok(Some(region))
  }

  pub fn parse_templex_atom_and_call_and_prefixes_and_suffixes(
    &self,
    original_iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<ITemplexPT<'p>> {
    let inner = self.parse_templex_atom_and_call_and_prefixes(original_iter)?;
    Ok(inner)
  }

  pub fn parse_templex_atom(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<ITemplexPT<'p>> {
    assert!(iter.peek_cloned().is_some());
    let _begin = iter.get_pos();

    if let Some(range) = iter.try_skip_word(self.keywords.underscore) {
      return Ok(ITemplexPT::AnonymousRune(AnonymousRunePT { range }));
    }
    if let Some(range) = iter.try_skip_word(self.keywords.truue) {
      return Ok(ITemplexPT::Bool(BoolPT { range, value: true }));
    }
    if let Some(range) = iter.try_skip_word(self.keywords.faalse) {
      return Ok(ITemplexPT::Bool(BoolPT { range, value: false }));
    }
    if let Some(proto) = self.parse_prototype(iter)? {
      return Ok(proto);
    }

    if let Some(tup) = self.parse_tuple(iter)? {
      return Ok(tup);
    }

    if let Some(array) = self.parse_array(iter)? {
      return Ok(array);
    }

    match iter.peek_cloned().expect("peek should not be empty") {
      INodeLEEnum::String(StringLE { range, parts }) => {
        iter.advance();
        match parts {
          [StringPart::Literal { range, s }] => {
            Ok(ITemplexPT::String(StringPT { range: *range, str: *s }))
          }
          _ => Err(ParseError::BadStringInTemplex(range.begin())),
        }
      }
      INodeLEEnum::ParsedInteger(ParsedIntegerLE { range, value, .. }) => {
        iter.advance();
        Ok(ITemplexPT::Int(IntPT { range, value }))
      }
      INodeLEEnum::ParsedDouble(ParsedDoubleLE { range, .. }) => {
        let pos = range.begin();
        iter.advance();
        Err(ParseError::RangedInternalError {
          pos,
          msg: "Floats in types not supported!".to_string(),
        })
      }
      INodeLEEnum::Word(WordLE { range, str }) => {
        iter.advance();
        Ok(ITemplexPT::NameOrRune(NameOrRunePT::new(NameP(range, str))))
      }
      _ => Err(ParseError::BadTypeExpression(iter.get_pos())),
    }
  }

  pub fn parse_template_call_args(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<&'p [&'p ITemplexPT<'p>]>> {
    let angled = match iter.peek_cloned() {
      Some(INodeLEEnum::Angled(a)) => a.clone(),
      Some(_) => return Ok(None),
      None => return Ok(None),
    };

    iter.advance();

    let mut elements_p: Vec<&'p ITemplexPT<'p>> = Vec::new();
    let angled_contents = angled.contents.clone();
    let contents_iter = ScrambleIterator::new(&angled_contents);
    let element_iters = contents_iter.split_on_symbol(',', false);

    for element_iter in element_iters {
      let mut elem_iter = element_iter.clone();
      elements_p.push(&*self.parse_arena.alloc(self.parse_templex(&mut elem_iter)?));
    }

    Ok(Some(self.parse_arena.alloc_slice_from_vec(elements_p)))
  }

  pub fn parse_tuple(
    &self,
    outer_iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<ITemplexPT<'p>>> {
    let _begin = outer_iter.get_pos();

    match outer_iter.peek_cloned() {
      Some(INodeLEEnum::Parend(ParendLE { range, contents })) => {
        let contents = contents.clone();
        outer_iter.advance();

        let mut elements: Vec<&'p ITemplexPT<'p>> = Vec::new();
        let contents_iter = ScrambleIterator::new(&contents);
        let iter_splits = contents_iter.split_on_symbol(',', false);

        for iter_split in iter_splits {
          let mut iter = iter_split.clone();
          elements.push(&*self.parse_arena.alloc(self.parse_templex(&mut iter)?));
        }

        Ok(Some(ITemplexPT::Tuple(TuplePT {
          range,
          elements: self.parse_arena.alloc_slice_from_vec(elements),
        })))
      }
      _ => Ok(None),
    }
  }

  pub fn parse_templex_atom_and_call(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<ITemplexPT<'p>> {
    let begin = iter.get_pos();

    let atom = self.parse_templex_atom(iter)?;

    match self.parse_template_call_args(iter)? {
      Some(args) => {
        return Ok(ITemplexPT::Call(CallPT {
          range: RangeL::new(begin, iter.get_prev_end_pos()),
          template: &*self.parse_arena.alloc(atom),
          args,
        }));
      }
      None => {}
    }

    Ok(atom)
  }

  pub fn parse_templex_atom_and_call_and_prefixes(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<ITemplexPT<'p>> {
    assert!(iter.has_next());

    match iter.peek_cloned() {
      Some(INodeLEEnum::Word(WordLE { str, .. })) if str == self.keywords.r#in => {
        panic!("Should not interpret 'in' as a valid templex");
      }
      _ => {}
    }

    let _begin = iter.get_pos();

    if let Some(x) = self.parse_ending_region(iter)? {
      return Ok(ITemplexPT::RegionRune(x));
    }

    if let Some(x) = self.parse_ref_prefix(iter)? {
      return Ok(x);
    }

    self.parse_templex_atom_and_call(iter)
  }

  pub fn parse_templex(&self, iter: &mut ScrambleIterator<'p, '_>) -> ParseResult<ITemplexPT<'p>> {
    self.parse_templex_atom_and_call_and_prefixes_and_suffixes(iter)
  }

  pub fn parse_typed_rune(
    &self,
    original_iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<IRulexPR<'p>>> {
    match original_iter.peek2_cloned() {
      (Some(INodeLEEnum::Word(WordLE { str: name_str, .. })), _)
        if name_str == self.keywords.func =>
      {
        Ok(None)
      }
      (
        Some(INodeLEEnum::Word(WordLE { range: name_range, str: name_str })),
        Some(INodeLEEnum::Word(WordLE { range: type_range, .. })),
      ) => {
        let maybe_name = if name_str == self.keywords.underscore {
          None
        } else {
          Some(NameP(name_range, name_str))
        };

        original_iter.advance();

        let tyype = match self.parse_rune_type(original_iter)? {
          None => panic!("Expected rune type"),
          Some(x) => x,
        };

        Ok(Some(IRulexPR::Typed(TypedPR {
          range: RangeL::new(name_range.begin(), type_range.end()),
          rune: maybe_name,
          tyype,
        })))
      }
      _ => Ok(None),
    }
  }

  pub fn parse_rule_call(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<IRulexPR<'p>>> {
    match iter.peek2_cloned() {
      (Some(INodeLEEnum::Word(WordLE { str, .. })), _) if str == self.keywords.func => {
        return Ok(None);
      }
      (
        Some(INodeLEEnum::Word(WordLE { range: name_range, str: name })),
        Some(INodeLEEnum::Parend(ParendLE { range: args_range, contents: args_lr })),
      ) => {
        let range = RangeL::new(name_range.begin(), args_range.end());

        let mut args_pr = Vec::new();
        let args_lr_clone = args_lr.clone();
        let args_iter = ScrambleIterator::new(&args_lr_clone);
        let arg_iters = args_iter.split_on_symbol(',', false);

        for arg_iter in arg_iters {
          let mut iter = arg_iter.clone();
          args_pr.push(self.parse_rule(&mut iter)?);
        }

        Ok(Some(IRulexPR::BuiltinCall(BuiltinCallPR {
          range,
          name: NameP(name_range, name),
          args: self.parse_arena.alloc_slice_from_vec(args_pr),
        })))
      }
      _ => Ok(None),
    }
  }

  pub fn parse_rule_atom(&self, iter: &mut ScrambleIterator<'p, '_>) -> ParseResult<IRulexPR<'p>> {
    let _begin = iter.get_pos();

    if let Some(x) = self.parse_rule_call(iter)? {
      return Ok(x);
    }

    if let Some(x) = self.parse_typed_rune(iter)? {
      return Ok(x);
    }

    let t = self.parse_templex(iter)?;
    Ok(IRulexPR::Templex(t))
  }

  pub fn parse_rule_up_to_equals_precedence(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<IRulexPR<'p>> {
    let maybe_before_iter = try_skip_past_equals_while(iter, |scouting_iter| {
      match scouting_iter.peek_cloned() {
        None => false,
        // Stop on comma
        Some(INodeLEEnum::Symbol(SymbolLE(_, ','))) => false,
        // Stop if we hit an open brace, its the function body
        Some(INodeLEEnum::Curlied(_)) => false,
        _ => true,
      }
    });

    match maybe_before_iter {
      None => {
        self.parse_rule_atom(iter)
      }
      Some(mut before_iter) => {
        let left = self.parse_rule_atom(&mut before_iter)?;
        let right = self.parse_rule_atom(iter)?;
        Ok(IRulexPR::Equals(EqualsPR {
          range: RangeL::new(left.range().begin(), right.range().end()),
          left: &*self.parse_arena.alloc(left),
          right: &*self.parse_arena.alloc(right),
        }))
      }
    }
  }

  pub fn parse_rule(&self, iter: &mut ScrambleIterator<'p, '_>) -> ParseResult<IRulexPR<'p>> {
    self.parse_rule_up_to_equals_precedence(iter)
  }

  pub fn parse_rune_type(
    &self,
    iter: &mut ScrambleIterator<'p, '_>,
  ) -> ParseResult<Option<ITypePR>> {
    match iter.peek_cloned() {
      None => Ok(None),

      Some(INodeLEEnum::Word(WordLE { str: w, .. })) if w == self.keywords.int_capitalized => {
        iter.advance();
        Ok(Some(ITypePR::IntType))
      }
      Some(INodeLEEnum::Word(WordLE { str: w, .. })) if w == self.keywords.region => {
        iter.advance();
        Ok(Some(ITypePR::RegionType))
      }
      Some(INodeLEEnum::Word(WordLE { str: w, .. })) if w == self.keywords.ref_list => {
        iter.advance();
        Ok(Some(ITypePR::CoordListType))
      }
      _ => Err(ParseError::BadRuneTypeError(iter.get_pos())),
    }
  }
}
