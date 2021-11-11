use snake::runner;

macro_rules! mk_test {
    ($test_name:ident, $file_name:expr, $expected_output:expr) => {
        #[test]
        fn $test_name() -> std::io::Result<()> {
            test_example_file($file_name, $expected_output)
        }
    };
}
macro_rules! mk_any_test {
    ($test_name:ident, $file_name:expr) => {
        #[test]
        fn $test_name() -> std::io::Result<()> {
            test_example_any($file_name)
        }
    };
}

macro_rules! mk_fail_test {
    ($test_name:ident, $file_name:expr, $expected_output:expr) => {
        #[test]
        fn $test_name() -> std::io::Result<()> {
            test_example_fail($file_name, $expected_output)
        }
    };
}

mk_any_test!(test, "_12.boa");
mk_test!(_12, "_12.boa", "-12");
mk_test!(three, "3.boa", "3");
mk_test!(add1, "add1.boa", "101");
mk_test!(bigconst, "bigconst.boa", "8589934592");
mk_test!(let_, "let.boa", "5");
mk_test!(easylet, "easylet.boa", "3");
mk_test!(chainedlet, "chainedlet.boa", "82");
mk_test!(minus_minus_boa, "minus_minus.boa", "-4");
mk_test!(op_minus, "op_minus.boa", "68");
mk_test!(op_plus, "op_plus.boa", "132");
mk_test!(op_tmies, "op_tmies.boa", "3200");
mk_test!(overflow32, "overflow32.boa", "4294967396");
mk_test!(parens_boa, "parens.boa", "42");
mk_test!(pparens_boa, "pparens.boa", "3");
mk_test!(precedence, "precedence.boa", "22");
mk_test!(shadow, "shadow.boa", "-3");
mk_test!(sub1, "sub1.boa", "99");
mk_test!(tree_minus, "tree_minus.boa", "8");
mk_test!(tree_plus, "tree_plus.boa", "33");
mk_test!(tree_times, "tree_times.boa", "165");
mk_test!(eq_false, "eq_false.cobra", "false");
mk_test!(eq_true, "eq_true.cobra", "true");
mk_test!(false_, "false.cobra", "false");
mk_test!(true_, "true.cobra", "true");
mk_test!(false_and_false, "false_and_false.cobra", "false");
mk_test!(false_and_true, "false_and_true.cobra", "false");
mk_test!(true_and_false, "true_and_false.cobra", "false");
mk_test!(true_and_true, "true_and_true.cobra", "true");
mk_test!(false_or_false, "false_or_false.cobra", "false");
mk_test!(false_or_true, "false_or_true.cobra", "true");
mk_test!(true_or_false, "true_or_false.cobra", "true");
mk_test!(true_or_true, "true_or_true.cobra", "true");
mk_test!(ge_false, "ge_false.cobra", "false");
mk_test!(ge_true, "ge_true.cobra", "true");
mk_test!(gt_equal, "gt_equal.cobra", "false");
mk_test!(gt_false, "gt_false.cobra", "false");
mk_test!(gt_true, "gt_true.cobra", "true");
mk_test!(le_false, "le_false.cobra", "false");
mk_test!(le_true, "le_true.cobra", "true");
mk_test!(lt_equal, "lt_equal.cobra", "false");
mk_test!(lt_false, "lt_false.cobra", "false");
mk_test!(lt_true, "lt_true.cobra", "true");
mk_test!(if_false, "if_false.cobra", "999");
mk_test!(if_true, "if_true.cobra", "4");
mk_test!(isbool_false, "isbool_false.cobra", "false");
mk_test!(isbool_true, "isbool_true.cobra", "true");
mk_test!(isnum_false, "isnum_false.cobra", "false");
mk_test!(isnum_true, "isnum_true.cobra", "true");
mk_test!(large_literal, "large_literal.cobra", "1073741823");
mk_test!(minus_minus, "minus_minus.cobra", "-4");
mk_test!(not_false, "not_false.cobra", "true");
mk_test!(not_true, "not_true.cobra", "false");
mk_test!(print_bool, "print_bool.cobra", "true\nfalse\ntrue");
mk_test!(print_num, "print_num.cobra", "40\n-70\n-30");
mk_test!(simple_add, "simple_add.cobra", "-1");
mk_test!(simple_mul, "simple_mul.cobra", "1152");
mk_test!(simple_sub, "simple_sub.cobra", "870");
mk_test!(tricky_mul, "tricky_mul.cobra", "1073741820");
mk_test!(_64bit, "64bit.diamond", "4294967295\n4294967294\ntrue");
mk_test!(arg_alias, "arg_alias.diamond", "10");
mk_test!(arg_alias2, "arg_alias2.diamond", "12");
mk_test!(
    arg_alias_print,
    "arg_alias_print.diamond",
    "12\n10\n10\n12\n10"
);
mk_test!(chained_let, "chained_let.diamond", "82");
mk_test!(eager_args, "eager_args.diamond", "18\n2\n38\n38");
mk_test!(even_odd, "even_odd.diamond", "true\nfalse\ntrue");
mk_test!(even_odd_simp, "even_odd_simp.diamond", "true");
mk_test!(fact, "fact.diamond", "120\n24\n6\n2\n1\n1");
mk_test!(if_in_let, "if_in_let.diamond", "23");
mk_test!(lazy_if, "lazy_if.diamond", "3");
mk_test!(let_in_fn, "let_in_fn.diamond", "10");
mk_test!(nested_call, "nested_call.diamond", "5\n-6");
mk_test!(no_args, "no_args.diamond", "3\n4");
mk_test!(no_fns, "no_fns.diamond", "false");
mk_test!(print_stack, "print_stack.diamond", "1\n120\n120\n120\n240");
mk_test!(ps0, "ps0.diamond", "0\n0");
mk_test!(simple_call, "simple_call.diamond", "9\n18");
mk_test!(simpler_call, "simpler_call.diamond", "3");
mk_test!(tail_arity, "tail_arity.diamond", "true");
mk_test!(tail_mutual, "tail_mutual.diamond", "true");
mk_test!(tail_rec, "tail_rec.diamond", "1000001");
mk_test!(tree_call, "tree_call.diamond", "5\n-9");
mk_test!(unused_fn, "unused_fn.diamond", "true");
mk_test!(use_retval, "use_retval.diamond", "-1");

// Condition Error
mk_fail_test!(err_cond_if_0, "+if.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_1, "comp_if_pos.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_2, "comp_if_zero.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_3, "if+.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_4, "if-in-let.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_5, "ifneg.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_6, "ifpos.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_7, "if0.boa", "if expected a boolean");
mk_fail_test!(err_cond_if_8, "if_12.cobra", "if expected a boolean");
mk_fail_test!(
    err_cond_if_9,
    "if_wanted_bool.diamond",
    "if expected a boolean"
); // TODO: print '1' then error

// Duplicate binding
mk_fail_test!(
    dup_bind,
    "dup_bind.boa",
    "Variable qux defined twice in let-expression"
);
mk_fail_test!(
    err_dup_let,
    "err_dup_let.diamond",
    "Variable x defined twice in let-expression"
);

// Unbound variable
mk_fail_test!(err0, "err0.boa", "Unbound variable x");
mk_fail_test!(unbound, "unbound.boa", "Unbound variable bar");
mk_fail_test!(x, "x.boa", "Unbound variable x_ffs_");

// Arithmetic Error
mk_fail_test!(
    false_times_0,
    "false_times_0.cobra",
    "arithmetic expected a number"
);
mk_fail_test!(
    arith_wanted_int,
    "arith_wanted_int.diamond",
    "arithmetic expected a number"
); // TODO: print '101' then error

// Logic Error
mk_fail_test!(not_1, "not_1.cobra", "logic expected a boolean");
mk_fail_test!(true_and_12, "true_and_12.cobra", "logic expected a boolean");
mk_fail_test!(true_and_3, "true_and_3.cobra", "logic expected a boolean");
mk_fail_test!(
    logic_wanted_bool,
    "logic_wanted_bool.diamond",
    "logic expected a boolean"
); // TODO: print 'true' then error

// Overflow Error
mk_fail_test!(overflow64, "overflow64.boa", "overflowed");
mk_fail_test!(overflow_add, "overflow_add.cobra", "overflowed");
mk_fail_test!(overflow_add1, "overflow_add1.cobra", "overflowed");
mk_fail_test!(overflow_mul, "overflow_mul.cobra", "overflowed");
mk_fail_test!(overflow_sub, "overflow_sub.cobra", "overflowed");
mk_fail_test!(overflow_sub1, "overflow_sub1.cobra", "overflowed");
mk_fail_test!(dyn_overflow, "dyn_overflow.diamond", "overflowed"); // TODO: print '3628800' then error
mk_fail_test!(
    err_static_overflow,
    "err_static_overflow.diamond",
    "Number literal 9223372036854775807 doesn't fit into 63-bit integer"
);

// Wrong arity
mk_fail_test!(
    err_arity,
    "err_arity.diamond",
    "function f of arity 3 called with 2 arguments"
);
mk_fail_test!(
    err_several,
    "err_several.diamond",
    "function even of arity 1 called with 0 arguments"
);

// Duplicate argument
mk_fail_test!(
    err_dup_arg,
    "err_dup_arg.diamond",
    "multiple arguments named \"foo\""
);

// Duplicate function name
mk_fail_test!(
    err_dup_fn,
    "err_dup_fn.diamond",
    "multiple defined functions named \"foo\""
);

// Undefined function
mk_fail_test!(
    err_unbound_fn,
    "err_unbound_fn.diamond",
    "Undefined function bar called"
);

// Undefined function variable
mk_fail_test!(
    err_unbound_var,
    "err_unbound_var.diamond",
    "function foo used in non-function call"
);

// Error Parsing
mk_fail_test!(weird, "weird.boa", "Unrecognized token `let`");
mk_fail_test!(parens, "parens.cobra", "Unrecognized EOF");

// IMPLEMENTATION
fn test_example_file(f: &str, expected_str: &str) -> std::io::Result<()> {
    use std::path::Path;
    let p_name = format!("examples/{}", f);
    let path = Path::new(&p_name);

    let tmp_dir = tempfile::TempDir::new()?;
    let mut w = Vec::new();
    match runner::compile_and_run_file(path, tmp_dir.path(), &mut w) {
        Ok(()) => {
            let stdout = std::str::from_utf8(&w).unwrap();
            assert_eq!(stdout.trim(), expected_str)
        }
        Err(e) => {
            panic!("Expected {}, got an error: {}", expected_str, e)
        }
    }
    Ok(())
}

fn test_example_any(f: &str) -> std::io::Result<()> {
    use std::path::Path;
    let p_name = format!("examples/{}", f);
    let path = Path::new(&p_name);

    let tmp_dir = tempfile::TempDir::new()?;
    let mut w = Vec::new();
    match runner::compile_and_run_file(path, tmp_dir.path(), &mut w) {
        Ok(()) => {}
        Err(e) => {
            panic!("Got an error: {}", e)
        }
    }
    Ok(())
}

fn test_example_fail(f: &str, includes: &str) -> std::io::Result<()> {
    use std::path::Path;

    let tmp_dir = tempfile::TempDir::new()?;
    let mut w_run = Vec::new();
    match runner::compile_and_run_file(
        Path::new(&format!("examples/{}", f)),
        tmp_dir.path(),
        &mut w_run,
    ) {
        Ok(()) => {
            let stdout = std::str::from_utf8(&w_run).unwrap();
            panic!("Expected a failure but got: {}", stdout.trim())
        }
        Err(e) => {
            let msg = format!("{}", e);
            assert!(
                msg.contains(includes),
                "Expected error message to include the string \"{}\" but got the error: {}",
                includes,
                msg
            )
        }
    }
    Ok(())
}
