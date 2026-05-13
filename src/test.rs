use std::collections::HashMap;

fn random_seed_gen() -> &'static str {
    "43"
}
fn main() {
    let mut results = HashMap::new();
    results.insert("name","Somesh");
    results.insert("age","31");
    results.insert("gender","male");
    results.insert("location","Bangalore");

    //Check the presence of a key
    if results.contains_key("name") {
        println!("Name = {}", results["name"]);
    }

    //Delete a key
    results.remove("location");

    println!("Does result contains location? {}", results.contains_key("location"));

    //Insert a key
    results.insert("location", "Remote");

    //Insert a key only if it doesn't already exist
    results.entry("location").or_insert("Hyderabad");
    results.entry("food").or_insert("non veg");

    //insert a key using a function which returns the value
    results.entry("id").or_insert_with(random_seed_gen);

    //Iterate over all the keys
    for (key, value) in &results {
        println!("Key: {0}, Value: {1}", key, value);
    }

}
