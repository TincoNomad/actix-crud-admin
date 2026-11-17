diesel::table! {
    products (id) {
        id -> Int4,
        name -> Varchar,
        price -> Float8,
        active -> Bool,
        category -> Varchar,
        created_at -> Timestamp,
    }
}
