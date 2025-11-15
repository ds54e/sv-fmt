use sv_fmt::config::FormatConfig;
use sv_fmt::formatter::format_text;

fn cfg() -> FormatConfig {
    FormatConfig::default()
}

#[test]
fn formats_basic_structure() {
    let input = "module top;
initial begin
if(a)b<=c;
else
c<=d;
end
endmodule
";
    let expected = "\
module top;
  initial begin
    if (a) b <= c;
    else
    c <= d;
  end
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn aligns_preprocessor_left() {
    let input = "module x;
  `ifdef FOO
    assign a = b,c,d;
  `else
foo ( bar );
  `endif
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    for line in formatted.lines() {
        if line.starts_with('`') {
            assert!(!line.starts_with(" "), "directive must be left aligned: {line}");
        }
    }
}

#[test]
fn call_and_comma_spacing() {
    let input = "module x;
initial begin
foo (a,b ,c);
end
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    assert!(formatted.contains("foo(a, b, c);"));
}

#[test]
fn inline_end_else_one_line() {
    let input = "module x;
initial begin
if (a) begin
  do_something();
end
else begin
  other();
end
end
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    assert!(
        formatted.contains("end else begin"),
        "expected inline end else. got:\n{formatted}"
    );
}

#[test]
fn wraps_multiline_blocks_when_enabled() {
    let input = "module x;
initial begin
if (cond)
  a <= 1;
  b <= 2;
end
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    let expected = "\
module x;
  initial begin
    if (cond)
    begin
      a <= 1;
      b <= 2;
    end
  end
endmodule
";
    assert_eq!(formatted, expected);
}

#[test]
fn does_not_wrap_case_statement_body() {
    let input = "module x;
always_comb begin
if (cond)
  case(sel)
    0: foo <= 1;
    default: foo <= 0;
  endcase
end
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    assert!(
        !formatted.contains("if (cond)\n    begin"),
        "case body should not trigger auto begin:\n{formatted}"
    );
}

#[test]
fn comment_spacing_rules() {
    let input = "module x;
initial begin
//leading
assign a = 1;   //  trailing
/* block comment */
assign b = 2;
end
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    assert!(
        formatted.contains("  //leading"),
        "leading comment should only have indent:\n{formatted}"
    );
    assert!(
        formatted.contains("assign a = 1; //  trailing"),
        "inline comment should have a single separator space:\n{formatted}"
    );
    assert!(
        formatted.contains("\n\n    /* block comment */\n\n"),
        "block comment should be surrounded by blank lines:\n{formatted}"
    );
}

#[test]
fn aligns_case_colons() {
    let input = "module x;
always_comb begin
case(sel)
  2'b0: foo = 0;
  4'b1010: foo = 1;
  default: foo = 2;
endcase
end
endmodule
";
    let formatted = format_text(input, &cfg()).unwrap();
    let short = formatted
        .lines()
        .find(|line| line.contains("foo = 0;"))
        .expect("missing short case item");
    assert!(
        short.contains("0    :"),
        "short label should be padded before colon:\n{formatted}"
    );
}

#[test]
fn adds_blank_lines_around_declarations() {
    let input = "package demo;
class foo;
endclass
class bar;
endclass
endpackage
interface baz();
endinterface
";
    let formatted = format_text(input, &cfg()).unwrap();
    let expected = "\
package demo;

  class foo;
  endclass

  class bar;
  endclass
endpackage

interface baz();
  endinterface
";
    assert_eq!(formatted, expected);
}

#[test]
fn auto_wraps_long_lines_when_enabled() {
    let input = "module x;
assign data = {foo, bar, baz, quux};
endmodule
";
    let mut cfg = FormatConfig::default();
    cfg.auto_wrap_long_lines = true;
    cfg.max_line_length = 20;
    let formatted = format_text(input, &cfg).unwrap();
    let mut lines = formatted.lines();
    let assign_line = lines.find(|line| line.contains("assign data")).unwrap();
    assert!(
        assign_line.contains("{foo,"),
        "first line should contain begin of concatenation:\n{formatted}"
    );
    let continuation = formatted
        .lines()
        .find(|line| line.trim_start().starts_with("bar, baz"))
        .expect("missing continuation line");
    assert!(
        continuation.starts_with("  ") || continuation.starts_with("\t"),
        "continuation line should be indented:\n{formatted}"
    );
}

#[test]
fn keeps_body_when_wrap_disabled() {
    let mut cfg = FormatConfig::default();
    cfg.wrap_multiline_blocks = false;
    let input = "module x;
initial begin
if (cond)
  a <= 1;
  b <= 2;
end
endmodule
";
    let formatted = format_text(input, &cfg).unwrap();
    assert!(
        !formatted.contains("if (cond)\n    begin"),
        "unexpected begin insertion:\n{formatted}"
    );
}
