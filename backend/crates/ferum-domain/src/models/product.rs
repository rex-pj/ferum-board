use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProductType {
    Furniture,
    Material,
    Room,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProductStatus {
    Draft,
    Published,
    Archived,
}

/// A reviewable subject: a furniture piece, a material, or a whole room.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub product_type: ProductType,
    pub status: ProductStatus,
    pub brand_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub style: Option<String>,
    /// Price bounds in the smallest currency unit (VND has no minor unit).
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub currency: String,
    /// Free-form spec object, e.g. `{ "w": 120, "d": 60, "h": 75, "unit": "cm" }`.
    pub dimensions: Value,
    pub origin: Option<String>,
    pub primary_image_key: Option<String>,
    pub description_md: Option<String>,
    pub created_by_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct NewProduct {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub product_type: ProductType,
    pub brand_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub style: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub currency: String,
    pub dimensions: Value,
    pub origin: Option<String>,
    pub description_md: Option<String>,
    pub created_by_id: Option<Uuid>,
    /// Material ids this product is made of (drives the product_materials join).
    pub material_ids: Vec<Uuid>,
}
