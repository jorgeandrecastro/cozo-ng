use cozo_ng_cleaner::{io, CleanError, Pipeline, Step, Table};
use serde_json::{json, Value};

fn csv(text: &str) -> Table {
    io::read_table(text, io::Format::Csv).unwrap()
}

fn run(toml: &str, input: Table) -> cozo_ng_cleaner::Output {
    Pipeline::from_toml(toml).unwrap().run(input).unwrap()
}

#[test]
fn standard_pipeline_cleans_dedups_and_nulls() {
    let t = csv("name,city\n  Ada   Lovelace ,London\nAda Lovelace,London\n,\nBob,N/A\n");
    let out = Pipeline::standard().run(t).unwrap();
    assert_eq!(out.table.rows, vec![vec![json!("Ada Lovelace"), json!("London")], vec![json!("Bob"), Value::Null]]);
    assert_eq!(out.report.rows_in, 4);
    assert_eq!(out.report.rows_out, 2);
    assert_eq!(out.report.rejected, 0);
}

#[test]
fn cast_reject_reports_original_row_numbers() {
    let t = csv("id,age\n1,30\n2,abc\n3,41\n");
    let out = run(
        r#"[[steps]]
op = "cast"
types = { id = "int", age = "int" }
on_error = "reject""#,
        t,
    );
    assert_eq!(out.table.rows.len(), 2);
    assert_eq!(out.rejects.len(), 1);
    assert_eq!(out.rejects[0].row, 2);
    assert_eq!(out.rejects[0].step, "cast");
    assert!(out.rejects[0].reason.contains("age"));
    assert_eq!(out.rejects[0].record["id"], json!("2"));
}

#[test]
fn cast_handles_separators_and_booleans() {
    let t = csv("n,x,b\n\"1,234\",\"1 234,5\",oui\n+7,2.50,no\n");
    let out = run(
        r#"[[steps]]
op = "cast"
types = { n = "int", b = "bool" }
[[steps]]
op = "cast"
types = { x = "float" }
decimal_comma = true"#,
        t,
    );
    // second row: "2.50" with decimal_comma drops the dot => 250.0 (documented behaviour)
    assert_eq!(out.table.rows[0], vec![json!(1234), json!(1234.5), json!(true)]);
    assert_eq!(out.table.rows[1][0], json!(7));
    assert_eq!(out.table.rows[1][2], json!(false));
}

#[test]
fn require_and_validate_reject_with_reasons() {
    let t = csv("id,price,email\n1,10,a@b.co\n,5,x@y.zz\n3,-4,c@d.ee\n4,7,not-an-email\n");
    let out = run(
        r#"[[steps]]
op = "cast"
types = { price = "float" }
[[steps]]
op = "require"
columns = ["id"]
[[steps]]
op = "validate"
rules = [
  { column = "price", min = 0 },
  { column = "email", pattern = '^[^@\s]+@[^@\s]+$' },
]"#,
        t,
    );
    assert_eq!(out.table.rows.len(), 1);
    let rows: Vec<usize> = out.rejects.iter().map(|r| r.row).collect();
    assert_eq!(rows, vec![2, 3, 4]);
    assert!(out.rejects[1].reason.contains("minimum"));
    assert_eq!(out.report.rejected, 3);
    // every input row is accounted for
    assert_eq!(out.report.rows_in, out.report.rows_out + out.report.rejected);
}

#[test]
fn dedup_keep_last_and_by_columns() {
    let t = csv("id,v\n1,a\n2,b\n1,c\n");
    let out = run("[[steps]]\nop = \"dedup\"\nby = [\"id\"]\nkeep = \"last\"", t);
    assert_eq!(out.table.rows, vec![vec![json!("2"), json!("b")], vec![json!("1"), json!("c")]]);
    assert_eq!(out.report.steps[0].rows_removed, 1);
}

#[test]
fn rename_select_drop_snake_case_fill() {
    let t = csv("First Name,Age,junk\nAda,,x\n");
    let out = run(
        r#"[[steps]]
op = "snake_case_headers"
[[steps]]
op = "nulls"
[[steps]]
op = "fill"
values = { age = 0 }
[[steps]]
op = "rename"
map = { first_name = "name" }
[[steps]]
op = "drop"
columns = ["junk"]
[[steps]]
op = "select"
columns = ["age", "name"]"#,
        t,
    );
    assert_eq!(out.table.columns, vec!["age", "name"]);
    assert_eq!(out.table.rows, vec![vec![json!(0), json!("Ada")]]);
}

#[test]
fn unknown_column_is_a_clear_error() {
    let err = Pipeline::from_toml("[[steps]]\nop = \"require\"\ncolumns = [\"nope\"]")
        .unwrap()
        .run(csv("a\n1\n"))
        .unwrap_err();
    assert_eq!(err, CleanError::UnknownColumn { step: "require".into(), column: "nope".into() });
}

#[test]
fn chunk_then_add_id_gives_unique_stable_ids() {
    let body = "First paragraph of the document, with a few words.\n\nSecond paragraph, a little longer than the first one, to force a cut.\n\nThird.";
    let t = Table::new(vec!["doc".into(), "body".into()], vec![vec![json!("d1"), json!(body)], vec![json!("d2"), Value::Null]]);
    let p = Pipeline::from_toml(
        r#"[[steps]]
op = "normalize_text"
whitespace = "keep_lines"
[[steps]]
op = "chunk"
column = "body"
size = 60
overlap = 10
[[steps]]
op = "add_id"
column = "chunk_id"
from = ["doc", "chunk_index"]
prefix = "c-""#,
    )
    .unwrap();
    let a = p.run(t.clone()).unwrap();
    let b = p.run(t).unwrap();
    assert_eq!(a, b, "deterministic");
    assert!(a.table.rows.len() >= 3);
    assert!(a.table.rows.iter().all(|r| r[0] == json!("d1")));
    let ids: std::collections::HashSet<_> = a.table.rows.iter().map(|r| r[3].as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), a.table.rows.len());
    assert!(ids.iter().all(|i| i.starts_with("c-") && i.len() == 18));
    // d2 had no text: rejected with its original row number
    assert_eq!(a.rejects.len(), 1);
    assert_eq!(a.rejects[0].row, 2);
    assert_eq!(a.report.steps[1].rows_rejected, 1);
}

#[test]
fn ragged_rows_are_refused() {
    let t = Table::new(vec!["a".into(), "b".into()], vec![vec![json!(1)]]);
    assert!(matches!(Pipeline::new().run(t), Err(CleanError::Parse(_))));
}

#[test]
fn builder_api_and_report_display() {
    let p = Pipeline::new().step(Step::normalize_text()).step(Step::DropEmptyRows);
    let out = p.run(csv("a,b\n x ,y\n,\n")).unwrap();
    let text = out.report.to_string();
    assert!(text.contains("rows: 2 in, 1 out, 0 rejected"), "{text}");
    assert!(text.contains("normalize_text"));
    assert!(text.contains("1 rows removed"), "{text}");
}

#[test]
fn json_input_keeps_types_and_writes_back() {
    let t = io::read_table("[{\"a\": 1, \"b\": \" x \"}, {\"a\": 2}]", io::Format::Json).unwrap();
    let out = Pipeline::new().step(Step::Trim { columns: vec![] }).run(t).unwrap();
    let mut buf = vec![];
    io::write_table(&out.table, io::Format::Jsonl, &mut buf).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), "{\"a\":1,\"b\":\"x\"}\n{\"a\":2,\"b\":null}\n");
}
