use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct CatalogResponse<T> {
    status: &'static str,
    currency: &'static str,
    items: T,
}

#[derive(Serialize)]
pub struct Region {
    code: &'static str,
    label: &'static str,
}

#[derive(Serialize)]
pub struct PurchaseOption {
    key: &'static str,
    label: &'static str,
    ahb: bool,
}

pub async fn aws_regions() -> Json<CatalogResponse<[Region; 3]>> {
    Json(CatalogResponse {
        status: "scaffold",
        currency: "USD",
        items: [
            Region {
                code: "eu-west-1",
                label: "EU (Ireland)",
            },
            Region {
                code: "eu-central-1",
                label: "EU (Frankfurt)",
            },
            Region {
                code: "us-east-1",
                label: "US East (N. Virginia)",
            },
        ],
    })
}

pub async fn purchase_options() -> Json<CatalogResponse<[PurchaseOption; 8]>> {
    Json(CatalogResponse {
        status: "static",
        currency: "USD",
        items: [
            PurchaseOption {
                key: "payg",
                label: "PAYG",
                ahb: false,
            },
            PurchaseOption {
                key: "ahb",
                label: "PAYG + Azure Hybrid Benefit",
                ahb: true,
            },
            PurchaseOption {
                key: "one-year",
                label: "1-Year Reserved",
                ahb: false,
            },
            PurchaseOption {
                key: "ahbone-year",
                label: "1-Year Reserved + AHB",
                ahb: true,
            },
            PurchaseOption {
                key: "three-year",
                label: "3-Year Reserved",
                ahb: false,
            },
            PurchaseOption {
                key: "ahbthree-year",
                label: "3-Year Reserved + AHB",
                ahb: true,
            },
            PurchaseOption {
                key: "sv-one-year",
                label: "1-Year Savings Plan",
                ahb: false,
            },
            PurchaseOption {
                key: "ahbsv-one-year",
                label: "1-Year Savings Plan + AHB",
                ahb: true,
            },
        ],
    })
}
