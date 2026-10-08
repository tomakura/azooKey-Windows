// A real subprocess used only to verify the external-provider protocol.
fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    assert_eq!(arguments.len(), 5);
    assert_eq!(arguments[1], "--reading");
    assert_eq!(arguments[3], "--context");
    match arguments[2].as_str() {
        "たいむあうと" => std::thread::sleep(std::time::Duration::from_secs(5)),
        "しっぱい" => {
            eprintln!("provider failure");
            std::process::exit(2);
        }
        "から" => {}
        _ => println!("{}：{}", arguments[4], arguments[2]),
    }
}
