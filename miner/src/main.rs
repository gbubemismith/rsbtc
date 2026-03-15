fn print_my_string(string: &String) {
    println!("{}", string);
}

fn main() {
    let the_example = String::from("Just an example");
    print_my_string(&the_example);

    let c = "Q";
    let ref ref_ci = c;
}
