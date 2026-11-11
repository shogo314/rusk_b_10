mod support;

use support::Case;

fn runs(source: &str, input: &str, expected: &str) {
    let case = Case::new(source);
    for options in [&[][..], &["--no-opt"][..]] {
        let output = case.run_with_options(input, options);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected.as_bytes());
    }
}

#[test]
fn float_uses_ordinary_shared_values_and_ieee_arithmetic() {
    runs(
        include_str!("../examples/beta09/float.rusk"),
        "",
        "0.00125 0.00125 0.0025\n2.500 -3 -2 -1 -2\n-0 -inf Yes No Yes\nYes\n",
    );
}

#[test]
fn float_tokens_patterns_and_input_round_trip() {
    runs(
        r#"
in a, b, c, d, e: Float;
wt a, b, c, d, e;
wt is_finite(a), is_inf(d), e < a, e <= a, e > a, e >= a;
wt Float(Str(a)) == a, Float(Str(b)) == b, 1.0 / Float(Str(c));
wt format_fixed(-a, 1000).len(), format_fixed(d, 3), format_fixed(c, 2);
let pattern := match -1.0e0 {0.0 => 0, -1e0 => 1, _ => 2,};
let zero := match -0.0 {0.0 => 1, _ => 0,};
wt pattern, zero, [1.0, 2E+0], (Float(3), 4e0);
let total := 0;
for i in 0..3 {total += i;}
wt total, Int(-9.223372036854776e18), try_read::<Float>().is_none();
"#,
        "1.7976931348623157e308 5e-324 -0 +inf nan",
        "1.7976931348623157e+308 5e-324 -0 inf nan\nYes Yes No No No No\nYes Yes -inf\n1311 inf -0.00\n1 1 1 2 3 4\n3 -9223372036854775808 Yes\n",
    );
}

#[test]
fn float_reports_bad_literals_and_missing_implicit_conversions() {
    for (source, expected) in [
        ("wt 1e400;", "Float literal out of range"),
        ("wt 1e-400;", "Float literal out of range"),
        ("let x:Float:=1;", "expected Float"),
        ("wt 1.0+2;", "no matching"),
    ] {
        let output = Case::new(source).compile();
        assert!(!output.status.success(), "{source}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:1:") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn invalid_float_input_conversions_and_precision_report_locations() {
    for (source, input, expected) in [
        ("in x:Float;", "1e-400", "Float out of range"),
        ("in x:Float;", "1e400", "Float out of range"),
        ("in x:Float;", "4e+", "invalid Float"),
        ("wt Float(\"0x1p2\");", "", "invalid Float"),
        (
            "wt Int(Float(\"inf\"));",
            "",
            "Float cannot be converted to Int",
        ),
        (
            "wt Int(Float(\"nan\"));",
            "",
            "Float cannot be converted to Int",
        ),
        (
            "wt Int(9.223372036854776e18);",
            "",
            "Float cannot be converted to Int",
        ),
        ("wt format_fixed(1.0, -1);", "", "Float precision"),
        ("wt format_fixed(1.0, 1001);", "", "Float precision"),
    ] {
        let output = Case::new(source).run(input);
        assert_eq!(output.status.code(), Some(1), "{source}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:1:") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn int128_uses_checked_floor_arithmetic_and_shared_values() {
    runs(
        include_str!("../examples/beta09/int128.rusk"),
        "",
        "-170141183460469231731687303715884105728 170141183460469231731687303715884105727 170141183460469231731687303715884105726 170141183460469231731687303715884105727\n-3 2 -3 -2\n0 7 3 4\nYes -42 -3 Yes\n",
    );
}

#[test]
fn int128_literal_context_patterns_and_decimal_io() {
    runs(
        r#"
fn identity(x:Int128)->Int128 {return x;}
fn from_literal()->Int128 {return +123;}
in a,b:Int128;
let pair:(Int128,Int):=(3,4);
let v:Vec<Int128>:=[1,-2,3];
wt a,b,identity(18446744073709551616),from_literal(),pair,v;
wt match a { -170141183460469231731687303715884105728 => 1, _ => 0, };
wt match 1i128 { 1i128 => 2, _ => 0, };
wt Int128(Float(a)) == a, Int128(true), !0i128;
fn select(x:Int)->Int {return 1;}
fn select(x:Int128)->Int {return 2;}
wt select(1),select(1i128);
"#,
        "-170141183460469231731687303715884105728 +170141183460469231731687303715884105727",
        "-170141183460469231731687303715884105728 170141183460469231731687303715884105727 18446744073709551616 123 3 4 1 -2 3\n1\n2\nYes 1 Yes\n1 2\n",
    );
}

#[test]
fn int128_invalid_literals_and_mixed_operands_are_diagnosed() {
    for (source, expected) in [
        (
            "wt 170141183460469231731687303715884105728i128;",
            "Int128 literal out of range",
        ),
        (
            "let x:Int128:=-170141183460469231731687303715884105729;",
            "Int128 literal out of range",
        ),
        ("wt 9223372036854775808;", "integer literal out of range"),
        ("let x:=1;let y:Int128:=x;", "expected Int128"),
        ("let x:=1;wt x+2i128;", "no matching"),
    ] {
        let output = Case::new(source).compile();
        assert_eq!(output.status.code(), Some(1), "{source}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:1:") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn int128_overflow_input_and_narrowing_report_locations() {
    for (source, input, expected) in [
        (
            "wt 170141183460469231731687303715884105727i128+1i128;",
            "",
            "Int128 overflow",
        ),
        (
            "wt -170141183460469231731687303715884105728i128-1i128;",
            "",
            "Int128 overflow",
        ),
        (
            "wt 9223372036854775808i128*18446744073709551616i128;",
            "",
            "Int128 overflow",
        ),
        (
            "wt -170141183460469231731687303715884105728i128 / -1i128;",
            "",
            "Int128 overflow",
        ),
        (
            "let x: Int128 := -170141183460469231731687303715884105728;wt -x;",
            "",
            "Int128 overflow",
        ),
        ("wt 1i128 % 0i128;", "", "division by zero"),
        ("wt Int(9223372036854775808i128);", "", "integer overflow"),
        (
            "wt Int128(Float(\"nan\"));",
            "",
            "Float cannot be converted to Int128",
        ),
        (
            "wt Int128(Float(170141183460469231731687303715884105727i128));",
            "",
            "Float cannot be converted to Int128",
        ),
        (
            "in x:Int128;",
            "-170141183460469231731687303715884105729",
            "Int128 out of range",
        ),
        ("in x:Int128;", "2e3", "invalid Int128"),
    ] {
        let output = Case::new(source).run(input);
        assert_eq!(output.status.code(), Some(1), "{source}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:1:") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn power_shifts_and_complement_preserve_precedence_and_fixed_width() {
    runs(
        include_str!("../examples/beta09/numeric_operators.rusk"),
        "",
        "512 -4 4 1\n0.125 1267650600228229401496703205376\n8 2 -2 -2 12\n-9223372036854775808 0 -1 0\n-170141183460469231731687303715884105728 0 -1 -1 -1\n",
    );
}

#[test]
fn power_evaluates_left_to_right_and_shifts_keep_nested_type_arguments() {
    runs(
        r#"
fn record(order: Int, digit: Int) -> Int {
    order *= 10;
    order += digit;
    return digit;
}
fn identity<T>(x: T) -> T { return x; }
let order := 0;
wt record(order, 2) ** record(order, 3) ** record(order, 1), order;
let matrix: Vec<Vec<Int>> := [[4]];
wt identity::<Vec<Vec<Int>>>(matrix)[0][0]>>1;
wt (-2) ** 63, (-9223372036854775808) ** 1, (-1i128) ** 9223372036854775807;
wt is_nan((-1.0) ** 0.5), (~ ~7), (~ ~7i128);
"#,
        "",
        "8 231\n2\n-9223372036854775808 -9223372036854775808 -1\nYes 7 7\n",
    );
}

#[test]
fn invalid_shift_spelling_and_checked_powers_report_locations() {
    for source in ["wt 1 < < 2;", "wt 1 > // gap\n> 2;"] {
        let output = Case::new(source).compile();
        assert_eq!(output.status.code(), Some(1));
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:1:") && error.contains("without whitespace"),
            "{error}"
        );
    }
    for (source, expected) in [
        ("wt 2 ** -1;", "negative integer exponent"),
        ("wt 2i128 ** -1;", "negative integer exponent"),
        ("wt 2 ** 63;", "integer overflow"),
        ("wt 2i128 ** 127;", "Int128 overflow"),
    ] {
        let output = Case::new(source).run("");
        assert_eq!(output.status.code(), Some(1));
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:1:") && error.contains(expected),
            "{error}"
        );
    }
}
