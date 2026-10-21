use regex::Regex;

#[test]
fn dbconn_captures() {
    let re = Regex::new(r"mongodb(?:\+srv)?://(?:([^:/\s]{1,100}):([^@/\s]{1,200})@)?([^/\s?]{1,100})").unwrap();
    let s = "mongodb+srv://admin:Xy9kQ2mZpL4rT7w@cluster0.mongodb.net/db";
    for caps in re.captures_iter(s) {
        println!("match={:?} u={:?} p={:?} h={:?}",
            caps.get(0).map(|m| m.as_str()),
            caps.get(1).map(|m| m.as_str()),
            caps.get(2).map(|m| m.as_str()),
            caps.get(3).map(|m| m.as_str()));
    }
}
