pub fn print_document(title: &str, body: &str) {
    println!();
    println!("--- BEGIN {} ---", title);
    println!("{}", body);
    println!("--- END {} ---", title);
    println!();
}
