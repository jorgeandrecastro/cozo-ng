use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Human-readable table (default on a terminal)
    Table,
    /// One JSON array of objects
    Json,
    /// One JSON object per line (default when piped)
    Ndjson,
    /// Comma-separated values
    Csv,
}

#[derive(Parser, Debug)]
#[command(
    name = "cozo-ng",
    version,
    about = "The Cozo-NG command-line tool",
    long_about = "Query, import, export, back up and explore Cozo-NG databases.\n\n\
        The database is given by a URI (--db or $COZO_DB):\n  \
        mem:                    in-memory, non-persistent\n  \
        sqlite:./app.db         SQLite file\n  \
        rocksdb:./data          RocksDB directory (needs the `rocksdb` feature)\n  \
        http://host:9070        a running cozo-bin server (use --token or $COZO_AUTH)\n  \
        <alias>                 an alias from ~/.config/cozo-ng/config.toml",
    propagate_version = true
)]
pub struct Cli {
    /// Database URI or alias
    #[arg(short, long, global = true, env = "COZO_DB", value_name = "URI")]
    pub db: Option<String>,

    /// Authentication token for remote servers (sent as `x-cozo-auth`)
    #[arg(long, global = true, env = "COZO_AUTH", hide_env_values = true, value_name = "TOKEN")]
    pub token: Option<String>,

    /// Output format [default: table on a terminal, ndjson when piped]
    #[arg(short, long, global = true, value_enum)]
    pub format: Option<Format>,

    /// Truncate table cells to N characters (0 = never)
    #[arg(long, global = true, default_value_t = 80, value_name = "N")]
    pub truncate: usize,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Run a CozoScript query ("-" reads the script from stdin)
    Query(QueryArgs),
    /// Run a CozoScript file
    Run(RunArgs),
    /// Interactive shell with multi-line input, history and completion
    Repl,
    /// List relations
    Relations,
    /// Inspect or dump the schema
    #[command(subcommand)]
    Schema(SchemaCmd),
    /// Import CSV / JSON / JSONL into a relation
    Import(ImportArgs),
    /// Export a relation
    Export(ExportArgs),
    /// Write a backup (SQLite file) of the database
    Backup { path: PathBuf },
    /// Restore a backup into the (empty) database
    Restore { path: PathBuf },
    /// Show the query plan of a script
    Explain(ExplainArgs),
    /// Health check: connectivity, size, relations and row counts
    Doctor,
    /// HNSW vector indices and similarity search
    #[command(subcommand)]
    Vec(VecCmd),
    /// Graph algorithms on an edge relation
    #[command(subcommand)]
    Graph(GraphCmd),
    /// Generate shell completions
    Completions { shell: Shell },
}

#[derive(Args, Debug)]
pub struct QueryArgs {
    /// The script, or "-" for stdin
    pub script: String,
    /// Bind a parameter, usable as $name in the script (value is JSON, else a string)
    #[arg(short, long = "param", value_name = "NAME=VALUE")]
    pub params: Vec<String>,
    /// Refuse scripts that write
    #[arg(long)]
    pub immutable: bool,
    /// Print the elapsed time on stderr
    #[arg(long)]
    pub timer: bool,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Script file ("-" for stdin)
    pub file: String,
    #[arg(short, long = "param", value_name = "NAME=VALUE")]
    pub params: Vec<String>,
    #[arg(long)]
    pub immutable: bool,
    #[arg(long)]
    pub timer: bool,
}

#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// The script to explain, or "-" for stdin
    pub script: String,
}

#[derive(Subcommand, Debug)]
pub enum SchemaCmd {
    /// Columns of a relation (or the list of relations)
    Show { relation: Option<String> },
    /// Print `:create` statements that recreate the schema (version it in git)
    Dump { relation: Option<String> },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum InputFormat {
    Csv,
    Json,
    Jsonl,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum PutOp {
    Put,
    Insert,
    Update,
    Ensure,
    Rm,
}

impl PutOp {
    pub fn keyword(self) -> &'static str {
        match self {
            PutOp::Put => "put",
            PutOp::Insert => "insert",
            PutOp::Update => "update",
            PutOp::Ensure => "ensure",
            PutOp::Rm => "rm",
        }
    }
}

#[derive(Args, Debug)]
pub struct ImportArgs {
    /// Input file ("-" for stdin)
    pub file: String,
    /// Target relation (must already exist)
    #[arg(short, long)]
    pub relation: String,
    /// Input format [default: from the file extension]
    #[arg(short = 'i', long, value_enum)]
    pub input_format: Option<InputFormat>,
    /// Rows per transaction
    #[arg(long, default_value_t = 1000)]
    pub batch: usize,
    /// Operation applied to each row
    #[arg(long, value_enum, default_value = "put")]
    pub op: PutOp,
    /// CSV delimiter
    #[arg(long, default_value_t = ',')]
    pub delimiter: char,
    /// CSV without a header row: columns are taken in relation order
    #[arg(long)]
    pub no_header: bool,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Relation to export
    #[arg(short, long)]
    pub relation: String,
    /// Comma-separated columns [default: all]
    #[arg(short, long, value_delimiter = ',')]
    pub columns: Vec<String>,
    /// Maximum number of rows
    #[arg(short, long)]
    pub limit: Option<usize>,
    /// Write to a file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Dtype {
    F32,
    F64,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Distance {
    L2,
    Cosine,
    #[value(name = "ip")]
    Ip,
}

#[derive(Subcommand, Debug)]
pub enum VecCmd {
    /// Create an HNSW index on a vector column
    CreateIndex {
        /// Relation holding the vectors
        relation: String,
        /// Index name
        #[arg(long, default_value = "hnsw")]
        name: String,
        /// Vector column
        #[arg(long)]
        field: String,
        /// Vector dimension
        #[arg(long)]
        dim: u32,
        #[arg(long, value_enum, default_value = "f32")]
        dtype: Dtype,
        #[arg(long, value_enum, default_value = "cosine")]
        distance: Distance,
        /// Max connections per node
        #[arg(long, default_value_t = 50)]
        m: u32,
        /// Candidate list size at build time
        #[arg(long, default_value_t = 200)]
        ef_construction: u32,
    },
    /// Nearest-neighbour search
    Search {
        /// Relation holding the vectors
        relation: String,
        /// Index name
        #[arg(long, default_value = "hnsw")]
        index: String,
        /// Query vector as a JSON array, e.g. "[0.1, 0.2]"
        #[arg(long, conflicts_with = "vector_file")]
        vector: Option<String>,
        /// File with the query vector (JSON array, or numbers separated by spaces/commas; "-" = stdin)
        #[arg(long)]
        vector_file: Option<String>,
        /// Number of neighbours
        #[arg(short, default_value_t = 5)]
        k: u32,
        /// Search candidate list size
        #[arg(long, default_value_t = 50)]
        ef: u32,
        /// Only return neighbours closer than this distance
        #[arg(long)]
        radius: Option<f64>,
        /// Extra CozoScript filter expression on the neighbours, e.g. "year > 2020"
        #[arg(long)]
        filter: Option<String>,
        /// Columns to return [default: every non-vector column]
        #[arg(short, long, value_delimiter = ',')]
        columns: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum GraphCmd {
    /// PageRank over an edge relation (first two columns are from/to)
    Pagerank {
        relation: String,
        #[arg(long, default_value_t = 20)]
        top: usize,
    },
    /// Nodes reachable from NODE within N hops
    Neighbors {
        relation: String,
        node: String,
        #[arg(long, default_value_t = 2)]
        hops: u32,
    },
    /// Shortest path (Dijkstra) between two nodes
    Path {
        relation: String,
        from: String,
        to: String,
    },
}
