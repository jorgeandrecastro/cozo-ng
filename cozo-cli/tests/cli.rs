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
