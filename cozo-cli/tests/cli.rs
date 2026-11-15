use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cozo(dir: &TempDir) -> Command {
    let mut c = Command::cargo_bin("cozo-ng").unwrap();
    c.env_remove("COZO_DB").env_remove("COZO_AUTH");
    c.env("HOME", dir.path()).env("XDG_CONFIG_HOME", dir.path()).env("XDG_DATA_HOME", dir.path());
    c.arg("--db").arg(format!("sqlite:{}", dir.path().join("t.db").display()));
    c
}

#[test]
fn query_in_memory_defaults_to_ndjson_when_piped() {
    let dir = TempDir::new().unwrap();
    Command::cargo_bin("cozo-ng")
        .unwrap()
        .env("HOME", dir.path())
        .env_remove("COZO_DB")
        .args(["query", "?[a, b] <- [[1, 'x']]"])
        .assert()
        .success()
        .stdout("{\"a\":1,\"b\":\"x\"}\n");
}

#[test]
fn errors_keep_cozo_diagnostics_and_fail() {
    let dir = TempDir::new().unwrap();
    cozo(&dir)
        .args(["query", "?[x, Y] := x = 1, y = x + 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("eval::unbound_symb_in_head").and(predicate::str::contains("Symbol 'Y'")));
}

#[test]
fn immutable_refuses_writes() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", "--immutable", ":create r {a: Int}"]).assert().failure();
}

#[test]
fn import_export_roundtrip_csv_and_schema_dump() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", ":create city {name: String => pop: Int, area: Float?}"]).assert().success();
    let csv = dir.path().join("c.csv");
    std::fs::write(&csv, "name,pop,area\nParis,2100000,105.4\nLyon,520000,\n").unwrap();
    cozo(&dir).args(["import", csv.to_str().unwrap(), "-r", "city"]).assert().success();
    cozo(&dir)
        .args(["export", "-r", "city", "-f", "csv"])
        .assert()
        .success()
        .stdout("name,pop,area\nLyon,520000,null\nParis,2100000,105.4\n");
    cozo(&dir)
        .args(["schema", "dump"])
        .assert()
        .success()
        .stdout(predicate::str::contains(":create city { name: String => pop: Int, area: Float? }"));
}

#[test]
fn import_rejects_unknown_column() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", ":create t {a: Int}"]).assert().success();
    let f = dir.path().join("x.jsonl");
    std::fs::write(&f, "{\"a\": 1, \"zzz\": 2}\n").unwrap();
    cozo(&dir)
        .args(["import", f.to_str().unwrap(), "-r", "t"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("zzz"));
}

#[test]
fn rejects_unsafe_identifiers() {
    let dir = TempDir::new().unwrap();
    cozo(&dir)
        .args(["export", "-r", "t{a}; :rm x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid identifier"));
}

#[test]
fn graph_commands() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", ":create e {a: String, b: String}"]).assert().success();
    cozo(&dir)
        .args(["query", "?[a,b] <- [['x','y'],['y','z'],['z','w']] :put e {a, b}"])
        .assert()
        .success();
    cozo(&dir)
        .args(["graph", "neighbors", "e", "x", "--hops", "2"])
        .assert()
        .success()
        .stdout("{\"node\":\"y\",\"hops\":1}\n{\"node\":\"z\",\"hops\":2}\n");
    cozo(&dir)
        .args(["graph", "path", "e", "x", "w"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[\"x\",\"y\",\"z\",\"w\"]"));
    cozo(&dir).args(["graph", "pagerank", "e", "--top", "1"]).assert().success();
}

#[test]
fn vector_search() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", ":create v {id: Int => emb: <F32; 2>}"]).assert().success();
    cozo(&dir)
        .args(["query", "?[id, emb] <- [[1,[1,0]],[2,[0,1]]] :put v {id => emb}"])
        .assert()
        .success();
    cozo(&dir).args(["vec", "create-index", "v", "--field", "emb", "--dim", "2"]).assert().success();
    cozo(&dir)
        .args(["vec", "search", "v", "--vector", "[0.9, 0.1]", "-k", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"id\":1"));
}

#[test]
fn repl_from_stdin_handles_multiline_scripts() {
    let dir = TempDir::new().unwrap();
    cozo(&dir)
        .arg("repl")
        .write_stdin("a[x] := x = 1\na[x] := x = 2\n?[count(x)] := a[x]\n.help\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"count(x)\":2"));
}

#[test]
fn doctor_reports_relations() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", ":create t {a: Int}"]).assert().success();
    cozo(&dir).args(["query", "?[a] <- [[1],[2],[3]] :put t {a}"]).assert().success();
    cozo(&dir).arg("doctor").assert().success().stdout(predicate::str::contains("relations    1"));
}

#[test]
fn completions_are_generated() {
    let dir = TempDir::new().unwrap();
    Command::cargo_bin("cozo-ng")
        .unwrap()
        .env("HOME", dir.path())
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cozo-ng"));
}

// ---- `clean` -------------------------------------------------------------------------------

#[test]
fn clean_standard_pipeline_to_stdout() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("in.csv");
    std::fs::write(&f, "name,city\n  Ada   Lovelace ,London\nAda Lovelace,London\n,\nBob,N/A\n").unwrap();
    cozo(&dir)
        .args(["clean", f.to_str().unwrap(), "-q"])
        .assert()
        .success()
        .stdout("{\"name\":\"Ada Lovelace\",\"city\":\"London\"}\n{\"name\":\"Bob\",\"city\":null}\n");
}

#[test]
fn clean_with_config_rejects_and_output_file() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("in.csv");
    std::fs::write(&f, "id,price\n1,10\n2,abc\n3,-5\n").unwrap();
    let cfg = dir.path().join("p.toml");
    std::fs::write(
        &cfg,
        "[[steps]]\nop = \"cast\"\ntypes = { id = \"int\", price = \"float\" }\non_error = \"reject\"\n\
         [[steps]]\nop = \"validate\"\nrules = [{ column = \"price\", min = 0 }]\n",
    )
    .unwrap();
    let out = dir.path().join("clean.jsonl");
    let rej = dir.path().join("rej.jsonl");
    cozo(&dir)
        .args(["clean", f.to_str().unwrap(), "-c", cfg.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .args(["--rejects", rej.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("rows: 3 in, 1 out, 2 rejected"));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "{\"id\":1,\"price\":10.0}\n");
    let rejects = std::fs::read_to_string(&rej).unwrap();
    assert_eq!(rejects.lines().count(), 2);
    assert!(rejects.contains("\"row\":2") && rejects.contains("\"row\":3"));
}

#[test]
fn clean_into_relation_types_values_from_the_schema() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", ":create item {id: Int => name: String, price: Float?}"]).assert().success();
    let f = dir.path().join("items.csv");
    std::fs::write(&f, "id,name,price\n1, Widget ,9.5\n1,Widget,9.5\n2,Gadget,N/A\n").unwrap();
    cozo(&dir).args(["clean", f.to_str().unwrap(), "--into", "item", "-q"]).assert().success();
    cozo(&dir)
        .args(["export", "-r", "item", "-f", "csv"])
        .assert()
        .success()
        .stdout("id,name,price\n1,Widget,9.5\n2,Gadget,null\n");
}

#[test]
fn clean_dry_run_writes_nothing_and_example_config_is_valid() {
    let dir = TempDir::new().unwrap();
    cozo(&dir).args(["query", ":create t {a: Int}"]).assert().success();
    let f = dir.path().join("t.csv");
    std::fs::write(&f, "a\n1\n2\n").unwrap();
    cozo(&dir).args(["clean", f.to_str().unwrap(), "--into", "t", "--dry-run"]).assert().success().stderr(predicate::str::contains("dry run"));
    cozo(&dir).args(["query", "?[count(a)] := *t{a}"]).assert().success().stdout("{\"count(a)\":0}\n");

    let example = cozo(&dir).args(["clean", "--example-config"]).assert().success().get_output().stdout.clone();
    let cfg = dir.path().join("example.toml");
    std::fs::write(&cfg, example).unwrap();
    std::fs::write(dir.path().join("x.csv"), "id,title\n1,a\n").unwrap();
    // the example casts `price`, which this file lacks: the error names the step and the column
    cozo(&dir)
        .args(["clean", dir.path().join("x.csv").to_str().unwrap(), "-c", cfg.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown column"));
}

#[test]
fn clean_into_unknown_relation_fails_early() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("a.csv");
    std::fs::write(&f, "a\n1\n").unwrap();
    cozo(&dir).args(["clean", f.to_str().unwrap(), "--into", "nope"]).assert().failure().stderr(predicate::str::contains("relation_not_found"));
}
