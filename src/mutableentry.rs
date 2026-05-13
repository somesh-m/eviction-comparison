use std::collections::HashMap;

fn main() {
    let mut score_board = HashMap::new();
    let stat = score_board.entry("somesh").or_insert(20);
    println!("Previous score: {:?}", stat);
    *stat += 2;
    println!("Current score: {:?}", stat);
    score_board.insert("ramesh", 10);
    score_board.entry("ramesh").and_modify(|ramesh| *ramesh += 2).or_insert(20);
    println!("Ramesh score: {:?}", score_board["ramesh"]);

    match score_board.entry("cookie") {
        std::collections::hash_map::Entry::Occupied(mut _entry) => {
            println!("Key is Present");
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            println!("Key is not present");
            score_board.insert("test", 10);
            entry.insert(20);
            println!("Cookie score: {:?}",score_board["cookie"]);
        }
    }
}
