use diesel::prelude::*;
use serde::Serialize;

#[derive(Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = crate::schema::products)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Product {
    pub id: i32,
    pub name: String,
    pub price: f64,
    pub active: bool,
    pub category: String,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::products)]
pub struct NewProduct {
    pub name: String,
    pub price: f64,
    pub active: bool,
    pub category: String,
}
