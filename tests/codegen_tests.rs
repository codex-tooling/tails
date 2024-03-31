extern crate inkwell;
extern crate tails;

#[cfg(test)]
mod tests {
  use pretty_assertions::assert_eq;
  use tails::{diagnostic, lexer, pass};

  const TESTS_FOLDER: &str = "tests";
  const BUG_CURRENT_FOLDER: &str = "the current directory should exist and be accessible";

  const BUG_FILE_READ: &str =
    "test source file should exist, be accessible, and its contents should be valid UTF-8";

  fn lex_and_filter(source_code: &str) -> diagnostic::Maybe<Vec<lexer::Token>> {
    let tokens = tails::lexer::Lexer::lex_all(source_code)?;

    // SAFETY: What about illegal tokens? Would it cause the parser to error?
    // Filter tokens to only include those that are relevant (ignore
    // whitespace, comments, etc.).
    let filtered_tokens = tokens
      .into_iter()
      .filter(|token| {
        !matches!(
          token.0,
          tails::lexer::TokenKind::Whitespace(_) | tails::lexer::TokenKind::Comment(_)
        )
      })
      .collect();

    Ok(filtered_tokens)
  }

  fn lower_file(
    source_file_contents: &str,
    qualifier: tails::symbol_table::Qualifier,
  ) -> diagnostic::Maybe<String> {
    let mut parser = tails::parser::Parser::new(lex_and_filter(source_file_contents)?);
    let module_result = parser.parse_module(qualifier.clone());

    let module = match module_result {
      Ok(unit) => unit,
      Err(diagnostics) => return Err(diagnostics),
    };

    let test_package = tails::ast::Package::from([(qualifier.clone(), module)]);
    let mut pass_manager = pass::PassManager::new(&test_package);

    pass_manager.add_all_passes();

    let pass_manager_run_result = pass_manager.run(parser.get_id_count());

    let diagnostics_helper = diagnostic::DiagnosticsHelper {
      diagnostics: pass_manager_run_result.diagnostics,
    };

    if diagnostics_helper.contains_errors() {
      return Err(diagnostics_helper.diagnostics);
    }

    // Ensure that no pass has unmet dependencies.
    for pass_result in &pass_manager_run_result.results {
      assert!(
        !matches!(pass_result.1, pass::PassResult::UnmetDependencies),
        "no pass should have unmet dependencies"
      );
    }

    let llvm_lowering_pass_result = pass_manager_run_result
      .results
      .get(&pass::PassId::LlvmLowering)
      .expect("backend output should have been produced if there were no error diagnostics");

    Ok(match llvm_lowering_pass_result {
      // OPTIMIZE: Consume result and avoid cloning.
      pass::PassResult::LlvmIrOutput(llvm_ir_output) => llvm_ir_output.to_owned(),
      _ => {
        unreachable!("backend output should have been produced if there were no error diagnostics")
      }
    })
  }

  fn run_test(name: &str, folder_name: &str) -> diagnostic::Maybe<String> {
    const FILENAME_EXTENSION: &str = "tails";

    let tests_path = std::env::current_dir()
      .expect(BUG_CURRENT_FOLDER)
      .join(TESTS_FOLDER);

    let source_file_contents = std::fs::read_to_string(
      tests_path
        .join(folder_name)
        .join(name)
        .with_extension(FILENAME_EXTENSION),
    )
    .expect(BUG_FILE_READ);

    let qualifier = tails::symbol_table::Qualifier {
      package_name: String::from(TESTS_FOLDER),
      // FIXME: File names need to conform to identifier rules.
      module_name: name.to_string(),
    };

    lower_file(&source_file_contents, qualifier).map(|output| output.trim().to_string())
  }

  fn run_passing_test(name: &str) {
    const LLVM_FILENAME_EXTENSION: &str = "ll";
    const EXPECTED_OUTPUT_FOLDER: &str = "expected_output";
    const INPUT_FOLDER: &str = "passing";

    let tests_path = std::env::current_dir()
      .expect(BUG_CURRENT_FOLDER)
      .join(TESTS_FOLDER);

    let tests_output_path = tests_path.join(EXPECTED_OUTPUT_FOLDER);

    let output_file_path = tests_output_path
      .join(name)
      .with_extension(LLVM_FILENAME_EXTENSION);

    let actual_output = run_test(name, INPUT_FOLDER)
      .expect("there should be no error diagnostics produced on a passing test");

    let expected_output = if output_file_path.exists() {
      std::fs::read_to_string(output_file_path)
        .expect("corresponding output file exists, but cannot be read")
        .trim()
        .to_string()
    }
    // If the expected output file does not exist, that is acceptable;
    // the output LLVM IR is irrelevant. For example, this could mean that
    // the test did not aim to test LLVM lowering directly, but instead
    // things like the type system, or earlier phases.
    else {
      return;
    };

    assert_eq!(expected_output, actual_output);
  }

  fn run_failing_test(name: &str, matcher: &dyn Fn(Vec<diagnostic::Diagnostic>) -> bool) {
    const FAILING_FOLDER: &str = "failing";

    match run_test(name, FAILING_FOLDER) {
      Ok(llvm_ir_output) => {
        println!("{}", llvm_ir_output);
        panic!("failing tests should not succeed");
      }
      Err(diagnostics) => {
        let matcher_result = matcher(diagnostics.clone());

        if !matcher_result {
          dbg!(diagnostics);
        }

        assert!(
          matcher_result,
          "failing test should produce expected diagnostics"
        );
      }
    }
  }

  macro_rules! define_passing_tests {
    ($($name:ident),* $(,)?) => {
      $(
        #[test]
        fn $name() {
          run_passing_test(stringify!($name));
        }
      )*
    };
  }

  macro_rules! define_failing_tests {
    ($($name:ident),* $(,)?) => {
      $(
        #[test]
        fn $name() {
          run_failing_test(stringify!($name), &|diagnostics| !diagnostics.is_empty());
        }
      )*
    };
  }

  define_passing_tests!(
    access,
    access_object_string,
    access_foreign_var,
    as_int_to_int,
    as_int_to_real,
    binary_op_arithmetic,
    binary_op_logical,
    binding,
    binding_hof,
    binding_literal,
    binding_nullptr,
    binding_unit,
    block,
    block_yield_binding,
    closure,
    closure_capture_binding,
    closure_capture_parameter,
    closure_capture_self_calling,
    closure_capture_object,
    closure_capture_multiple,
    closure_capture_return_closure,
    closure_type_hint_constraint,
    // closure_binding_no_redefine,
    closure_self_call,
    closure_return,
    constant,
    // factorial,
    // fibonacci,
    foreign,
    foreign_var,
    foreign_var_object_type,
    foreign_varargs,
    function_empty,
    function_param,
    function_return,
    guard_division_by_zero,
    guard_memo,
    guard_null_dereference,
    hof,
    hof_args,
    hof_return,
    if_,
    if_else,
    if_elif,
    if_elif_else,
    if_values,
    if_nesting,
    if_nested_condition,
    if_complex_condition,
    infer_binary_op,
    infer_signature,
    sizeof,
    literals,
    // loop_,
    // loop_closure,
    // loop_range,
    declare,
    name_tick,
    object,
    object_nested,
    object_field_shorthand,
    object_call_pass_binding,
    playground,
    pipe,
    pipe_chain,
    pointer,
    pointer_assignment,
    pointer_assignment_foreign,
    pointer_index,
    match_,
    reference,
    reference_object,
    simple_program,
    tuple_typed,
    tuple_nested,
    tuple_single,
    tuple_indexing_simple,
    tuple_indexing_nested,
    type_infer_binding,
    type_infer_parameter,
    type_infer_object_access,
    type_infer_return_type,
    type_infer_complex,
    type_def,
    type_def_nested,
    unary_op,
    union,
    unit_constant,
    unit_if,
    unit_parameter_mixed,
    unit_parameter_multiple,
    unit_parameter_single,
    unit_object_fields,
    unit_closure_capture,
    unit_pointer_index,
    unit_tuple,
    vector,
    recursion_mutual,
    recursion_trio,
    lex_unicode_string,
    // REVISE: Better and more specific name for this test. It was related to the logic bug that would cause the unsafe flag to be false after leaving an unsafe scope, meaning that nested unsafe scopes may lead into issues (ie. if there's an unsafe action after a nested unsafe scope).
    semantics_unsafe_edge_case
  );

  define_failing_tests!(
    parser_function_missing_name,
    parser_trailing_comma,
    lex_unicode_unsupported,
    type_def_recursive,
    type_def_recursive_usage,
    type_def_mutually_recursive,
    type_def_mutually_recursive_usage,
    type_def_recursive_nested,
    call_argument_count,
    reference_return,
    object_missing_field,
    constant_runtime_value,
    declare_parameter_redefine,
    declare_parameter_redefine_function,
    declare_binding_redefine,
    declare_function_redefine,
    declare_foreign_function_redefine,
    call_site_invalid_direct_callee,
    call_site_invalid_indirect_callee,
    resolution_missing_function,
    type_infer_mismatch
  );
}
