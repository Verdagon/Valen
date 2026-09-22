use crate::code_source::CodeSource;
use crate::compile_options::GlobalOptions;
use crate::keywords::Keywords;
use crate::lexing::ast::RangeL;
use crate::lexing::errors::FailedParse;
use crate::parse_arena::ParseArena;
use crate::parsing::ast::FileP;
use crate::postparsing::ast::ProgramS;
use crate::postparsing::post_parser::ICompileErrorS;
use crate::postparsing::ScoutCompilation;
use crate::scout_arena::ScoutArena;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_humanizer::humanize;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::hinputs_t::HinputsT;
use crate::typing::oracles::Oracles;
use crate::typing::typing_interner::TypingInterner;
use crate::utils::code_hierarchy::FileCoordinateMap;
use crate::utils::code_hierarchy::PackageCoordinate;
use crate::utils::fx::HashMap;
use crate::utils::source_code_utils::humanize_pos_code_map;
use crate::utils::source_code_utils::line_containing;
use crate::utils::source_code_utils::line_range_containing;
use crate::utils::source_code_utils::lines_between;
use bumpalo::Bump;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct TypingPassOptions {
  pub global_options: GlobalOptions,
  pub debug_out: Arc<dyn Fn(&str) + Send + Sync>,
  pub tree_shaking_enabled: bool,
  pub borrow_checker_enabled: bool,
}

pub struct TypingPassCompilation<'s, 'ctx, 't, 'p>
where
  's: 't,
{
  scout_compilation: ScoutCompilation<'s, 'ctx, 'p>,
  hinputs_cache: Option<HinputsT<'s, 't>>,
  // VCOORD: we have this because it contains the postparseds table. lets consider moving
  // the postparseds table into HinputsT
  coutputs_cache: Option<CompilerOutputs<'s, 't>>,
  scout_arena: &'ctx ScoutArena<'s>,
  keywords: &'ctx Keywords<'s>,
  options: TypingPassOptions,
  pub typing_interner: &'ctx TypingInterner<'s, 't>,
  oracles: Oracles<'ctx, 's, 't>,
}

impl<'s, 'ctx, 't, 'p> TypingPassCompilation<'s, 'ctx, 't, 'p>
where
  's: 't,
{
  pub fn new(
    typing_interner: &'ctx TypingInterner<'s, 't>,
    scout_arena: &'ctx ScoutArena<'s>,
    keywords: &'ctx Keywords<'s>,
    parser_keywords: &'ctx Keywords<'p>,
    parse_arena: &'ctx ParseArena<'p>,
    packages_to_build: Vec<&'p PackageCoordinate<'p>>,
    code_source: &'ctx CodeSource<'p>,
    typing_options: TypingPassOptions,
    oracles: Oracles<'ctx, 's, 't>,
  ) -> Self {
    let scout_compilation = ScoutCompilation::new(
      scout_arena,
      keywords,
      parser_keywords,
      parse_arena,
      packages_to_build,
      code_source,
      typing_options.global_options.clone(),
    );

    TypingPassCompilation {
      scout_compilation,
      hinputs_cache: None,
      coutputs_cache: None,
      scout_arena,
      keywords,
      options: typing_options,
      typing_interner,
      oracles,
    }
  }

  pub fn get_code_map(&mut self) -> Result<FileCoordinateMap<'p, String>, FailedParse<'p>> {
    self.scout_compilation.get_code_map()
  }

  pub fn scout_arena_for_tests(&self) -> &'ctx ScoutArena<'s> {
    self.scout_arena
  }

  pub fn get_parseds(
    &mut self,
  ) -> Result<FileCoordinateMap<'p, (FileP<'p>, Vec<RangeL>)>, FailedParse<'p>> {
    self.scout_compilation.get_parseds()
  }

  pub fn get_vpst_map(&mut self) -> Result<FileCoordinateMap<'p, String>, FailedParse<'p>> {
    self.scout_compilation.get_vpst_map()
  }

  pub fn get_scoutput(
    &mut self,
  ) -> Result<&FileCoordinateMap<'s, ProgramS<'s>>, ICompileErrorS<'s>> {
    self.scout_compilation.get_scoutput()
  }

  // VTRACE: hide
  pub fn get_compiler_outputs(&mut self) -> Result<&HinputsT<'s, 't>, ICompileErrorT<'s, 't>> {
    if self.hinputs_cache.is_some() {
      return Ok(self.hinputs_cache.as_ref().unwrap());
    }
    let code_map = self.get_code_map().expect("getCodeMap failed");
    let astrouts = self.scout_compilation.expect_scoutput();
    let compiler = Compiler::new(
      self.scout_arena,
      &self.typing_interner,
      self.keywords,
      &self.options,
      self.oracles,
    );
    match compiler.evaluate(&code_map, astrouts) {
      Err(e) => Err(e),
      Ok((hinputs, coutputs)) => {
        self.hinputs_cache = Some(hinputs);
        self.coutputs_cache = Some(coutputs);
        Ok(self.hinputs_cache.as_ref().unwrap())
      }
    }
  }

  pub fn cached_coutputs(&self) -> &CompilerOutputs<'s, 't> {
    self.coutputs_cache.as_ref().expect("compiler outputs not computed")
  }

  // VTRACE: hide
  pub fn expect_compiler_outputs(&mut self) -> &HinputsT<'s, 't> {
    match self.get_compiler_outputs().err() {
      Some(err) => {
        let code_map = self.get_code_map().expect("getCodeMap failed");
        let error_text = humanize(
          self.scout_arena,
          self.typing_interner,
          true,
          &|x| humanize_pos_code_map(&code_map, &x),
          &|a, b| lines_between(&code_map, &a, &b),
          &|x| line_range_containing(&code_map, &x),
          &|x| line_containing(&code_map, &x),
          err,
        );
        panic!("{}", error_text);
      }

      None => self.hinputs_cache.as_ref().unwrap(),
    }
  }

  pub fn cached_compiler_outputs(&self) -> &HinputsT<'s, 't> {
    self.hinputs_cache.as_ref().expect("compiler outputs not computed")
  }
}
