extern crate ini;
use ini::Ini;

// Consider replacing docopt with clap
extern crate docopt;
use docopt::Docopt;
#[macro_use]
extern crate serde_derive;

extern crate serde_json;
use serde_json::Value;

extern crate rusqlite;
use rusqlite::Connection;

extern crate rustyline;
use rustyline::Editor;

extern crate clipboard;
use clipboard::ClipboardProvider;
use clipboard::ClipboardContext;

extern crate rand;
use rand::Rng;
use rand::os::OsRng;

use std::env;
use std::io;
use std::io::prelude::*;
use std::path::{PathBuf, Path};
use std::process::Command;

const INI_PATH: &'static str = ".pwrc";

const USAGE: &'static str = "
Command-line password manager using Keybase for cloud storage mechanism.
You must be logged in to Keybase.

Usage:
  pw add [<category>] <name>
  pw edit <name>
  pw delete <name>
  pw list
  pw list categories
  pw list <category>
  pw show <name>
  pw copy <name> (u|p)
  pw generate [--chars=<num>]
  pw -h | --help | --version
  pw --comp-name | --comp-sec


Options:
  -h --help      Show this screen.
  --version      Show version.
  --chars=<num>  Number of characters to generate [default: 32]
  --comp-name    List credential names for tab completion
  --comp-sec     List categories for tab completion

";

const CHAR_ALPHA: &'static str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const CHAR_NUM: &'static str = "1234567890";
const CHAR_SYMBOL: &'static str = "!@#$%^&*()";

#[derive(Debug, Deserialize)]
struct Args {
    cmd_add: bool,
    cmd_list: bool,
    cmd_categories: bool,
    cmd_show: bool,
    cmd_copy: bool,
    cmd_edit: bool,
    cmd_delete: bool,
    cmd_generate: bool,

    cmd_u: bool,
    cmd_p: bool,

    flag_comp_name: bool,
    flag_comp_sec: bool,
    flag_chars: usize,

    arg_name: String,
    arg_category: Option<String>
}

#[derive(Debug)]
struct Credential {
    id: u32,
    name: String,
    category: String,
    username: String,
    password: String
}

fn main() {

    let config = parse_config_file();
    let path = config.section(None::<String>).unwrap().get("datastore_path").unwrap();
    let conn = initialize_datastore(path);

    let args: Args = Docopt::new(USAGE)
                            .and_then(|d| d.deserialize())
                            .unwrap_or_else(|e| e.exit());
    //println!("{:?}", args);

    if args.cmd_add {
        new_credential(&conn, args.arg_category, args.arg_name);
    }
    else if args.cmd_list {
        if args.cmd_categories {
            list_categories(&conn);
        } else {
            list_credentials(&conn, args.arg_category)
        }
    }
    else if args.cmd_show {
        show_credential(&conn, args.arg_name);
    }
    else if args.cmd_copy {
        copy_credential(&conn, args.arg_name, args.cmd_u);
    }
    else if args.cmd_edit {
        edit_credential(&conn, args.arg_name);
    }
    else if args.cmd_delete {
        delete_credential(&conn, args.arg_name);
    }
    else if args.cmd_generate {
        generate_password(args.flag_chars);
    }
    else if args.flag_comp_name {
        completion_name(&conn);
    }
    else if args.flag_comp_sec {
        completion_sec(&conn);
    }

}

fn new_credential(conn: &rusqlite::Connection, category: Option<String>, name: String) {
    let category = match category {
        Some(c) => c,
        None => "".to_string()
    };
    // To make tab completion reasonable, we replace spaces with underscores in name and category
    let name = name.replace(" ", "_");
    let category = category.replace(" ", "_");

    if category == "categories" {
        println!("You cannot use the category 'categories'.");
        return;
    }
    if name_exists(conn, &name) {
        println!("A credential with this name already exists.");
        return;
    }

    print!("Creating new credentials named \"{}\"", name);
    if category != "" {
        print!(" in category \"{}\"", category)
    }
    println!("");

    let mut rl = Editor::<()>::new();
    let username = rl.readline("Username: ").expect("No username supplied.");
    let password = rl.readline("Password: ").expect("No password supplied.");
    conn.execute("INSERT INTO credentials
        (name, category, username, password)
        values
        (?1, ?2, ?3, ?4)",
        &[&name, &category, &username, &password]
    ).unwrap();

    println!("Saved.");
}

fn list_categories(conn: &rusqlite::Connection) {
    let mut statement = conn.prepare("SELECT DISTINCT(category) FROM credentials ORDER BY category").unwrap();
    let mut rows = statement.query(&[]).unwrap();
    println!("Categories:");
    while let Some(result_row) = rows.next() {
        let row = result_row.unwrap();
        let category: String = row.get(0);
        println!("    {}", category);
    }
}

fn list_credentials(conn: &rusqlite::Connection, category: Option<String>) {
    // TODO: how do we differentiate yes/no category in a non-stupid way?
    // lifetimes means `statement` must not go out of scope before `rows`
    let mut statement = match category.to_owned() {
        None => conn.prepare("SELECT category, name FROM credentials ORDER BY category,name").unwrap(),
        _ => conn.prepare("SELECT category, name FROM credentials WHERE category = ?1 ORDER BY category,name").unwrap()
    };
    let mut rows = match category.to_owned() {
        None => statement.query(&[]).unwrap(),
        _ => statement.query(&[&category]).unwrap()
    };

    // TODO: how do we do this in a non-stupid way?
    let mut previous_category = "".to_string();
    while let Some(result_row) = rows.next() {
        let row = result_row.unwrap();
        let category: String = row.get(0);
        let name: String = row.get(1);
        if previous_category != category {

            println!("Category: {}", category);
            previous_category = category;
        }
        println!("    {}", name);
    }
}

fn completion_name(conn: &rusqlite::Connection) {
    let mut statement = conn.prepare("SELECT name FROM credentials").unwrap();
    let mut rows = statement.query(&[]).unwrap();
    while let Some(result_row) = rows.next() {
        let entry: String = result_row.unwrap().get(0);
        print!("{} ", entry);
    }
}

fn completion_sec(conn: &rusqlite::Connection) {
    let mut statement = conn.prepare("SELECT distinct(category) FROM credentials").unwrap();
    let mut rows = statement.query(&[]).unwrap();
    while let Some(result_row) = rows.next() {
        let entry: String = result_row.unwrap().get(0);
        print!("{} ", entry);
    }
}

fn show_credential(conn: &rusqlite::Connection, name: String) {
    let credential = get_credential(conn, name);
    println!("{}:\n    {}\n    {}", credential.name, credential.username, credential.password);
}

fn get_credential(conn: &rusqlite::Connection, name: String) -> Credential {
    conn.query_row("SELECT * FROM credentials WHERE name = ?1", &[&name], |row| {
        Credential {
            id: row.get(0),
            name: row.get(1),
            category: row.get(2),
            username: row.get(3),
            password: row.get(4)
        }
    }).expect("No such credential saved.")
}

fn copy_credential(conn: &rusqlite::Connection, name: String, username: bool) {
    let credential = get_credential(conn, name);
    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
    if username {
        ctx.set_contents(credential.username).expect("Unable to write to clipboard.");
        println!("{} username copied to clipboard.", credential.name);
    } else {
        ctx.set_contents(credential.password).expect("Unable to write to clipboard.");
        println!("{} password copied to clipboard.", credential.name);
    }
    pause("(press enter to clear)");
}

fn edit_credential(conn: &rusqlite::Connection, name: String) {
    let credential = get_credential(conn, name);

    let mut rl = Editor::<()>::new();
    let name: String = match rl.readline(&format!("Name [{}]: ", credential.name)) {
        Ok(v) => if v == "" {credential.name} else {v},
        _ => credential.name
    };
    let category: String = match rl.readline(&format!("Category [{}]: ", credential.category)) {
        Ok(v) => if v == "" {credential.category} else {v},
        _ => credential.category
    };
    let username: String = match rl.readline(&format!("Username [{}]: ", credential.username)) {
        Ok(v) => if v == "" {credential.username} else {v},
        _ => credential.username
    };
    let password: String = match rl.readline(&format!("Password [{}]: ", credential.password)) {
        Ok(v) => if v == "" {credential.password} else {v},
        _ => credential.password
    };
    conn.execute("UPDATE credentials SET name=?1, category=?2, username=?3, password=?4 where id=?5",
        &[&name, &category, &username, &password, &credential.id]).expect("Unable to edit credential.");
    println!("Credential edited.");
}

fn delete_credential(conn: &rusqlite::Connection, name: String) {
    let credential = get_credential(conn, name);
    println!("Name: {}\nCategory: {}\nUsername: {}\n", credential.name, credential.category, credential.username);

    println!("Are you sure you wish to delete this credential?");
    let mut rl = Editor::<()>::new();
    match rl.readline(&format!("y/n [n]: ")) {
        Ok(v) => {
            if v == "y" {
                conn.execute("DELETE FROM  credentials WHERE id=?1", &[&credential.id]).expect("Unable to delete credential.");
                println!("Credential deleted.");
            } else {
                println!("Canceled.");
            }
        },
        _ => println!("Canceled.")
    };
}

fn generate_password(num_chars: usize) {
    let mut rand = match OsRng::new() {
        Ok(g) => g,
        Err(e) => panic!("Failed to obtain OS RNG: {}", e)
    };
    println!("Generating {}-character password:", num_chars);

    //let password = rand.gen_ascii_chars().take(num_chars).collect::<String>();
    let haystack = CHAR_ALPHA.to_owned() + CHAR_NUM + CHAR_SYMBOL;
    let mut pw = Vec::new();
    for _ in 1..num_chars {
        pw.push(*rand.choose(haystack.as_bytes()).unwrap());
    }
    let password = String::from_utf8(pw).unwrap();

    println!("    {}", password);
}

fn initialize_datastore(data_path: &str) -> rusqlite::Connection {
    let path = Path::new(data_path);
    let db_exists = path.is_file();
    let conn = Connection::open(path).unwrap();
    if !db_exists {
        conn.execute("CREATE TABLE credentials (
                      id              INTEGER PRIMARY KEY,
                      name            TEXT UNIQUE NOT NULL,
                      category        TEXT,
                      username        TEXT,
                      password        TEXT
                      )", &[]).unwrap();
    }
    return conn;
}


fn name_exists(conn: &rusqlite::Connection, name: &str) -> bool {
    match conn.query_row("SELECT count(*) FROM credentials WHERE name = ?1", &[&name], |row| {
            let val: i64 = row.get(0);
            val
        }) {
        Ok(0) => return false,
        _ => return true
    }
}

fn pause(message: &str) {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    print!("{}", message);
    stdout.flush().unwrap();

    let _ = stdin.read(&mut [0u8]).unwrap();
}

fn get_datastore_path() -> PathBuf {
    // note: querying keybase takes about 100 ms
    let keybase_query = Command::new("keybase")
        .arg("status")
        .arg("-j")
        .output()
        .expect("Unable to query Keybase -- is it installed?");
    if !keybase_query.status.success() {
        panic!("Keybase did not execute successfully.");
    }
    let data: Value = serde_json::from_slice(&keybase_query.stdout).expect("Unable to process Keybase output.");
    if data["LoggedIn"] != Value::Bool(true) {
        panic!("You are not logged in to Keybase.");
    }
    if data["KBFS"]["Running"] != Value::Bool(true) {
        panic!("You do not have KBFS enabled.");
    }
    match data["Username"] {
        Value::String(ref v) => {
            let mut data_path = PathBuf::from("/keybase/private/");
            data_path.push(v);
            data_path.push("pw.dat");
            data_path
        },
        _ => panic!("Unable to determine Keybase username.")
    }
}

fn parse_config_file() -> Ini {
    let mut config_path = env::home_dir().expect("Could not find your home directory.");
    config_path.push(INI_PATH);
    let config_path_str = config_path.to_str().unwrap();

    if let Ok(ini) = Ini::load_from_file(config_path_str) {
        if let Some(_) = ini.to_owned().section(None::<String>).unwrap().get("datastore_path") {
            return ini;
        }
    }
    return create_default_config(config_path_str);
}

fn create_default_config(path_to_write: &str) -> Ini {
    let datastore_path = get_datastore_path();
    let mut conf = Ini::new();
    conf.with_section(None::<String>).set("datastore_path", datastore_path.to_str().unwrap());
    conf.write_to_file(path_to_write).expect("Unable to write config file");
    conf
}