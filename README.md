# github-db

A simple rust library that allows you to use a github repository as a somewhat document based database inspired by mongodb.

**Don't Use If You...**
 - Need high write frequency from multiple users on the same collection.
 - Will be managing large amounts of data.
 - Will be storing user's personal data.

**Use If You...**
 - Need a temporary place to store data in the cloud between clients.
 - Don't write to the database very often.

## Example
```rust
use std::env;

use github_db::{Client, Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Review {
    name: String,
    review: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        env::var("TOKEN")?, // github token with repository read/write abilities
        "grantshandy", // a github username
        "testdb", // the repository you want to use
        None, // an optional alternate github api location
        Some("data/".to_string()), // an optional path prefix
    )?;

    // a collection of reviews in our database
    let mut books = client.collection::<Review>("reviews").await?;
    
    let reviews = vec![
        Review {
            name: "Adventures of Huckleberry Finn".to_string(),
            review: "My favorite book.".to_string(),
        },
        Review {
            name: "Grimms' Fairy Tales".to_string(),
            review: "Masterpiece.".to_string(),
        },
        Review {
            name: "Pride and Prejudice".to_string(),
            review: "Very enjoyable.".to_string(),
        },
    ];

    // overwrite all the data in the collection
    books.set_as(reviews).await?;

    // insert a single review
    books
        .insert(Review {
            name: "Pride and Prejudice".to_string(),
            review: "it was alright".to_string(),
        })
        .await?;

    // make all the names uppercase
    //
    // this isn't the most elegant but since
    // github doesn't have any logic of its
    // own we have to do this sort of thing
    // locally.
    let mapped = books
        .data()
        .await?
        .iter()
        .map(|r| Review {
            name: r.name.clone().to_uppercase(),
            review: r.review.clone(),
        })
        .collect();
    books.set_as(mapped).await?;

    // give us all the reviews with the name "PRIDE AND PREJUDICE"
    let select_reviews: Vec<&Review> = books
        .data()
        .await?
        .iter()
        .filter(|r| r.name == "PRIDE AND PREJUDICE")
        .collect();

    println!("{:#?}", select_reviews);

    Ok(())
}
```

```
[
    Review {
        name: "PRIDE AND PREJUDICE",
        review: "Very enjoyable.",
    },
    Review {
        name: "PRIDE AND PREJUDICE",
        review: "it was alright",
    },
]
```