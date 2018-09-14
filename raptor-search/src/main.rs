use std::env;
use std::fs::File;
use std::path::Path;
use std::collections::HashSet;
use std::str::from_utf8_unchecked;
use std::io::{self, BufReader, BufRead, Write};
use elapsed::measure_time;
use fst::Streamer;
use rocksdb::{DB, DBOptions, IngestExternalFileOptions};
use raptor::{automaton, Metadata, RankedStream};

type CommonWords = HashSet<String>;

fn common_words<P>(path: P) -> io::Result<CommonWords>
where P: AsRef<Path>,
{
    let file = File::open(path)?;
    let file = BufReader::new(file);
    let mut set = HashSet::new();
    for line in file.lines().filter_map(|l| l.ok()) {
        for word in line.split_whitespace() {
            set.insert(word.to_owned());
        }
    }
    Ok(set)
}

fn search(metadata: &Metadata, database: &DB, common_words: &CommonWords, query: &str) {
    let mut automatons = Vec::new();
    for query in query.split_whitespace().filter(|q| !common_words.contains(*q)) {
        let lev = automaton::build(query);
        automatons.push(lev);
    }

    let mut stream = RankedStream::new(&metadata, automatons, 20);
    while let Some(document) = stream.next() {
        print!("{:?}", document.document_id);

        let title_key = format!("{}-title", document.document_id);
        let title = database.get(title_key.as_bytes()).unwrap().unwrap();
        let title = unsafe { from_utf8_unchecked(&title) };
        print!(" {:?}", title);

        println!();
    }
}

fn main() {
    let name = env::args().nth(1).expect("Missing meta file name (e.g. lucid-ptolemy)");
    let map_file = format!("{}.map", name);
    let idx_file = format!("{}.idx", name);
    let sst_file = format!("{}.sst", name);

    let rocksdb = "rocksdb/storage";

    let (elapsed, meta) = measure_time(|| unsafe {
        Metadata::from_paths(map_file, idx_file).unwrap()
    });
    println!("{} to load metadata", elapsed);

    let (elapsed, db) = measure_time(|| {
        let db = DB::open_default(rocksdb).unwrap();
        db.ingest_external_file(&IngestExternalFileOptions::new(), &[&sst_file]).unwrap();
        drop(db);
        DB::open_for_read_only(DBOptions::default(), rocksdb, false).unwrap()
    });
    println!("{} to load the SST file in RocksDB and reopen it for read-only", elapsed);

    let common_path = "fr.stopwords.txt";
    let common_words = common_words(common_path).unwrap_or_else(|e| {
        println!("{:?}: {:?}", common_path, e);
        HashSet::new()
    });

    loop {
        print!("Searching for: ");
        io::stdout().flush().unwrap();

        let mut query = String::new();
        io::stdin().read_line(&mut query).unwrap();
        let query = query.trim().to_lowercase();

        if query.is_empty() { break }

        let (elapsed, _) = measure_time(|| search(&meta, &db, &common_words, &query));
        println!("Finished in {}", elapsed);
    }
}
