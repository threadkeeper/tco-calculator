use reqwest::Url;

use super::provider::ProviderError;

pub const PARSER_SCHEMA_VERSION: &str = "pricing-v2";

const EC2_CALCULATOR_BASE: &str =
    "https://calculator.aws/pricing/2.0/meteredUnitMaps/ec2/USD/current/ec2-calc/";
const EBS_METER_MAP_BASE: &str =
    "https://b0.p.awsstatic.com/pricing/2.0/meteredUnitMaps/ec2/USD/current/";
const AWS_PRICING_BASE: &str = "https://pricing.us-east-1.amazonaws.com/";
const AZURE_RETAIL_BASE: &str = "https://prices.azure.com/api/retail/prices";
const AZURE_CALCULATOR_URL: &str =
    "https://azure.microsoft.com/api/v3/pricing/azure-sql/calculator/?culture=en-us&discount=mca";
const AZURE_RETAIL_PAGE_SIZE: u64 = 1_000;
pub(crate) const AZURE_RETAIL_MAX_PAGES: usize = 32;
const AZURE_RETAIL_MAX_SKIP: u64 = (AZURE_RETAIL_MAX_PAGES as u64 - 1) * AZURE_RETAIL_PAGE_SIZE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AwsRegionScope {
    pub code: &'static str,
    pub location: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AzureRegionScope {
    pub arm_name: &'static str,
    pub calculator_slug: &'static str,
    pub display_name: &'static str,
}

pub const AZURE_REGION_SCOPES: [AzureRegionScope; 28] = [
    AzureRegionScope {
        arm_name: "australiaeast",
        calculator_slug: "australia-east",
        display_name: "Australia East",
    },
    AzureRegionScope {
        arm_name: "australiasoutheast",
        calculator_slug: "australia-southeast",
        display_name: "Australia Southeast",
    },
    AzureRegionScope {
        arm_name: "brazilsouth",
        calculator_slug: "brazil-south",
        display_name: "Brazil South",
    },
    AzureRegionScope {
        arm_name: "canadacentral",
        calculator_slug: "canada-central",
        display_name: "Canada Central",
    },
    AzureRegionScope {
        arm_name: "canadaeast",
        calculator_slug: "canada-east",
        display_name: "Canada East",
    },
    AzureRegionScope {
        arm_name: "centralindia",
        calculator_slug: "central-india",
        display_name: "Central India",
    },
    AzureRegionScope {
        arm_name: "centralus",
        calculator_slug: "us-central",
        display_name: "Central US",
    },
    AzureRegionScope {
        arm_name: "eastasia",
        calculator_slug: "asia-pacific-east",
        display_name: "East Asia",
    },
    AzureRegionScope {
        arm_name: "eastus",
        calculator_slug: "us-east",
        display_name: "East US",
    },
    AzureRegionScope {
        arm_name: "eastus2",
        calculator_slug: "us-east-2",
        display_name: "East US 2",
    },
    AzureRegionScope {
        arm_name: "francecentral",
        calculator_slug: "france-central",
        display_name: "France Central",
    },
    AzureRegionScope {
        arm_name: "germanywestcentral",
        calculator_slug: "germany-west-central",
        display_name: "Germany West Central",
    },
    AzureRegionScope {
        arm_name: "italynorth",
        calculator_slug: "italy-north",
        display_name: "Italy North",
    },
    AzureRegionScope {
        arm_name: "japaneast",
        calculator_slug: "japan-east",
        display_name: "Japan East",
    },
    AzureRegionScope {
        arm_name: "japanwest",
        calculator_slug: "japan-west",
        display_name: "Japan West",
    },
    AzureRegionScope {
        arm_name: "northcentralus",
        calculator_slug: "us-north-central",
        display_name: "North Central US",
    },
    AzureRegionScope {
        arm_name: "northeurope",
        calculator_slug: "europe-north",
        display_name: "North Europe",
    },
    AzureRegionScope {
        arm_name: "polandcentral",
        calculator_slug: "poland-central",
        display_name: "Poland Central",
    },
    AzureRegionScope {
        arm_name: "qatarcentral",
        calculator_slug: "qatar-central",
        display_name: "Qatar Central",
    },
    AzureRegionScope {
        arm_name: "southcentralus",
        calculator_slug: "us-south-central",
        display_name: "South Central US",
    },
    AzureRegionScope {
        arm_name: "southeastasia",
        calculator_slug: "asia-pacific-southeast",
        display_name: "Southeast Asia",
    },
    AzureRegionScope {
        arm_name: "swedencentral",
        calculator_slug: "sweden-central",
        display_name: "Sweden Central",
    },
    AzureRegionScope {
        arm_name: "switzerlandnorth",
        calculator_slug: "switzerland-north",
        display_name: "Switzerland North",
    },
    AzureRegionScope {
        arm_name: "uksouth",
        calculator_slug: "united-kingdom-south",
        display_name: "UK South",
    },
    AzureRegionScope {
        arm_name: "westcentralus",
        calculator_slug: "us-west-central",
        display_name: "West Central US",
    },
    AzureRegionScope {
        arm_name: "westeurope",
        calculator_slug: "europe-west",
        display_name: "West Europe",
    },
    AzureRegionScope {
        arm_name: "westus",
        calculator_slug: "us-west",
        display_name: "West US",
    },
    AzureRegionScope {
        arm_name: "westus2",
        calculator_slug: "us-west-2",
        display_name: "West US 2",
    },
];

pub fn aws_region_scope(region: &str) -> Result<AwsRegionScope, ProviderError> {
    let location = match region {
        "eu-central-1" => "EU (Frankfurt)",
        "eu-central-2" => "EU (Zurich)",
        "eu-north-1" => "EU (Stockholm)",
        "eu-south-1" => "EU (Milan)",
        "eu-south-2" => "EU (Spain)",
        "eu-west-1" => "EU (Ireland)",
        "eu-west-2" => "EU (London)",
        "eu-west-3" => "EU (Paris)",
        _ => return Err(ProviderError::Unsupported),
    };
    Ok(AwsRegionScope {
        code: match region {
            "eu-central-1" => "eu-central-1",
            "eu-central-2" => "eu-central-2",
            "eu-north-1" => "eu-north-1",
            "eu-south-1" => "eu-south-1",
            "eu-south-2" => "eu-south-2",
            "eu-west-1" => "eu-west-1",
            "eu-west-2" => "eu-west-2",
            "eu-west-3" => "eu-west-3",
            _ => unreachable!(),
        },
        location,
    })
}

pub fn azure_region_scope(region: &str) -> Result<AzureRegionScope, ProviderError> {
    AZURE_REGION_SCOPES
        .iter()
        .copied()
        .find(|scope| scope.arm_name == region)
        .ok_or(ProviderError::Unsupported)
}

pub fn ec2_metadata_url() -> Result<Url, ProviderError> {
    provider_url(EC2_CALCULATOR_BASE, &["metadata.json"])
}

pub fn ec2_selector_url(scope: AwsRegionScope) -> Result<Url, ProviderError> {
    provider_url(
        EC2_CALCULATOR_BASE,
        &[scope.location, "primary-selector-aggregations.json"],
    )
}

pub fn ec2_leaf_url(
    scope: AwsRegionScope,
    preinstalled_software: &str,
) -> Result<Url, ProviderError> {
    if !matches!(preinstalled_software, "NA" | "SQL Std" | "SQL Ent") {
        return Err(ProviderError::Unsupported);
    }
    provider_url(
        EC2_CALCULATOR_BASE,
        &[
            scope.location,
            "OnDemand",
            "Shared",
            "Windows",
            preinstalled_software,
            "No License required",
            "Yes",
            "index.json",
        ],
    )
}

pub fn ebs_meter_map_url() -> Result<Url, ProviderError> {
    provider_url(EBS_METER_MAP_BASE, &["ebs-calculator.json"])
}

pub fn aws_region_index_url(offer_code: &str) -> Result<Url, ProviderError> {
    if !matches!(
        offer_code,
        "AmazonEC2" | "AmazonRDS" | "AmazonRDSOCPULicenseFees"
    ) {
        return Err(ProviderError::Unsupported);
    }
    provider_url(
        AWS_PRICING_BASE,
        &[
            "offers",
            "v1.0",
            "aws",
            offer_code,
            "current",
            "region_index.json",
        ],
    )
}

pub fn aws_region_offer_url(
    scope: AwsRegionScope,
    offer_code: &str,
    current_version_url: &str,
) -> Result<Url, ProviderError> {
    if !matches!(offer_code, "AmazonRDS" | "AmazonRDSOCPULicenseFees")
        || !current_version_url.starts_with('/')
        || current_version_url.contains(['?', '#'])
    {
        return Err(ProviderError::Unsupported);
    }
    let segments = current_version_url[1..].split('/').collect::<Vec<_>>();
    let valid = matches!(
        segments.as_slice(),
        ["offers", "v1.0", "aws", path_offer, version, path_region, "index.json"]
            if *path_offer == offer_code
                && *path_region == scope.code
                && version.len() == 14
                && version.chars().all(|character| character.is_ascii_digit())
    );
    if !valid {
        return Err(ProviderError::Unsupported);
    }
    provider_url(AWS_PRICING_BASE, &segments)
}

pub fn azure_retail_url(scope: AzureRegionScope, currency: &str) -> Result<Url, ProviderError> {
    azure_retail_url_for(scope, currency, AzureRetailService::SqlManagedInstance)
}

pub fn azure_vm_retail_url(scope: AzureRegionScope, currency: &str) -> Result<Url, ProviderError> {
    azure_retail_url_for(scope, currency, AzureRetailService::VirtualMachines)
}

pub fn azure_managed_disk_retail_url(
    scope: AzureRegionScope,
    currency: &str,
) -> Result<Url, ProviderError> {
    azure_retail_url_for(scope, currency, AzureRetailService::ManagedDisks)
}

fn azure_retail_url_for(
    scope: AzureRegionScope,
    currency: &str,
    service: AzureRetailService,
) -> Result<Url, ProviderError> {
    if currency != "USD" {
        return Err(ProviderError::Unsupported);
    }
    let mut url = Url::parse(AZURE_RETAIL_BASE).map_err(|_| ProviderError::Unsupported)?;
    let filter = azure_retail_filter(scope, service);
    url.query_pairs_mut()
        .append_pair("currencyCode", currency)
        .append_pair("$filter", &filter);
    Ok(url)
}

pub fn azure_retail_continuation_url(
    scope: AzureRegionScope,
    currency: &str,
    source_url: &str,
) -> Result<Url, ProviderError> {
    azure_retail_continuation_url_for(
        scope,
        currency,
        source_url,
        AzureRetailService::SqlManagedInstance,
    )
}

pub fn azure_vm_retail_continuation_url(
    scope: AzureRegionScope,
    currency: &str,
    source_url: &str,
) -> Result<Url, ProviderError> {
    azure_retail_continuation_url_for(
        scope,
        currency,
        source_url,
        AzureRetailService::VirtualMachines,
    )
}

pub fn azure_managed_disk_retail_continuation_url(
    scope: AzureRegionScope,
    currency: &str,
    source_url: &str,
) -> Result<Url, ProviderError> {
    azure_retail_continuation_url_for(
        scope,
        currency,
        source_url,
        AzureRetailService::ManagedDisks,
    )
}

fn azure_retail_continuation_url_for(
    scope: AzureRegionScope,
    currency: &str,
    source_url: &str,
    service: AzureRetailService,
) -> Result<Url, ProviderError> {
    if currency != "USD" {
        return Err(ProviderError::Unsupported);
    }
    let url = Url::parse(source_url).map_err(|_| ProviderError::Unsupported)?;
    if url.scheme() != "https"
        || url.host_str() != Some("prices.azure.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || url.path() != "/api/retail/prices"
        || url.fragment().is_some()
    {
        return Err(ProviderError::Unsupported);
    }

    let expected_filter = azure_retail_filter(scope, service);
    let mut currency_count = 0_usize;
    let mut filter_count = 0_usize;
    let mut skip_count = 0_usize;
    for (name, value) in url.query_pairs() {
        match name.as_ref() {
            "currencyCode" if value == currency => currency_count += 1,
            "$filter" if value == expected_filter => filter_count += 1,
            "$skip"
                if !value.is_empty()
                    && value.chars().all(|character| character.is_ascii_digit())
                    && value.parse::<u64>().is_ok_and(|skip| {
                        (AZURE_RETAIL_PAGE_SIZE..=AZURE_RETAIL_MAX_SKIP).contains(&skip)
                            && skip % AZURE_RETAIL_PAGE_SIZE == 0
                    }) =>
            {
                skip_count += 1;
            }
            _ => return Err(ProviderError::Unsupported),
        }
    }
    if (currency_count, filter_count, skip_count) != (1, 1, 1) {
        return Err(ProviderError::Unsupported);
    }
    Ok(url)
}

pub fn azure_calculator_url() -> Result<Url, ProviderError> {
    Url::parse(AZURE_CALCULATOR_URL).map_err(|_| ProviderError::Unsupported)
}

#[derive(Clone, Copy)]
enum AzureRetailService {
    SqlManagedInstance,
    VirtualMachines,
    ManagedDisks,
}

fn azure_retail_filter(scope: AzureRegionScope, service: AzureRetailService) -> String {
    match service {
        AzureRetailService::SqlManagedInstance => format!(
            "serviceName eq 'SQL Managed Instance' and armRegionName eq '{}'",
            scope.arm_name
        ),
        AzureRetailService::VirtualMachines => format!(
            "serviceName eq 'Virtual Machines' and armRegionName eq '{}'",
            scope.arm_name
        ),
        AzureRetailService::ManagedDisks => format!(
            "serviceName eq 'Storage' and armRegionName eq '{}' and (productName eq 'Premium SSD Managed Disks' or productName eq 'Azure Premium SSD v2')",
            scope.arm_name
        ),
    }
}

fn provider_url(base: &str, segments: &[&str]) -> Result<Url, ProviderError> {
    let mut url = Url::parse(base).map_err(|_| ProviderError::Unsupported)?;
    url.path_segments_mut()
        .map_err(|_| ProviderError::Unsupported)?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_are_limited_to_reviewed_priceable_regions() {
        assert_eq!(
            aws_region_scope("eu-west-1").expect("reviewed AWS region"),
            AwsRegionScope {
                code: "eu-west-1",
                location: "EU (Ireland)"
            }
        );
        assert_eq!(
            azure_region_scope("swedencentral").expect("reviewed Azure region"),
            AzureRegionScope {
                arm_name: "swedencentral",
                calculator_slug: "sweden-central",
                display_name: "Sweden Central"
            }
        );
        assert_eq!(AZURE_REGION_SCOPES.len(), 28);
        let unique_arm_names = AZURE_REGION_SCOPES
            .iter()
            .map(|scope| scope.arm_name)
            .collect::<std::collections::BTreeSet<_>>();
        let unique_calculator_slugs = AZURE_REGION_SCOPES
            .iter()
            .map(|scope| scope.calculator_slug)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique_arm_names.len(), AZURE_REGION_SCOPES.len());
        assert_eq!(unique_calculator_slugs.len(), AZURE_REGION_SCOPES.len());
        assert_eq!(
            aws_region_scope("us-east-1"),
            Err(ProviderError::Unsupported)
        );
        assert_eq!(
            azure_region_scope("southafricanorth"),
            Err(ProviderError::Unsupported)
        );
        assert_eq!(
            azure_region_scope("chinanorth3"),
            Err(ProviderError::Unsupported)
        );
    }

    #[test]
    fn source_urls_encode_provider_scopes() {
        let aws = aws_region_scope("eu-west-1").expect("AWS scope");
        assert_eq!(
            ec2_selector_url(aws).expect("EC2 selector URL").as_str(),
            "https://calculator.aws/pricing/2.0/meteredUnitMaps/ec2/USD/current/ec2-calc/EU%20(Ireland)/primary-selector-aggregations.json"
        );
        assert_eq!(
            ec2_leaf_url(aws, "NA")
                .expect("EC2 compute leaf URL")
                .as_str(),
            "https://calculator.aws/pricing/2.0/meteredUnitMaps/ec2/USD/current/ec2-calc/EU%20(Ireland)/OnDemand/Shared/Windows/NA/No%20License%20required/Yes/index.json"
        );
        assert_eq!(
            ec2_leaf_url(aws, "SQL Std")
                .expect("EC2 SQL Standard leaf URL")
                .as_str(),
            "https://calculator.aws/pricing/2.0/meteredUnitMaps/ec2/USD/current/ec2-calc/EU%20(Ireland)/OnDemand/Shared/Windows/SQL%20Std/No%20License%20required/Yes/index.json"
        );
        assert_eq!(
            ec2_leaf_url(aws, "SQL Web"),
            Err(ProviderError::Unsupported)
        );
        assert_eq!(
            ebs_meter_map_url().expect("EBS meter map URL").as_str(),
            "https://b0.p.awsstatic.com/pricing/2.0/meteredUnitMaps/ec2/USD/current/ebs-calculator.json"
        );
        assert_eq!(
            aws_region_index_url("AmazonRDS")
                .expect("RDS region index URL")
                .as_str(),
            "https://pricing.us-east-1.amazonaws.com/offers/v1.0/aws/AmazonRDS/current/region_index.json"
        );
        assert_eq!(
            aws_region_offer_url(
                aws,
                "AmazonRDS",
                "/offers/v1.0/aws/AmazonRDS/20260806022930/eu-west-1/index.json"
            )
            .expect("RDS regional offer URL")
            .as_str(),
            "https://pricing.us-east-1.amazonaws.com/offers/v1.0/aws/AmazonRDS/20260806022930/eu-west-1/index.json"
        );
        for invalid in [
            "//example.invalid/offers/v1.0/aws/AmazonRDS/index.json",
            "/offers/v1.0/aws/AmazonRDS/not-a-version/eu-west-1/index.json",
            "/offers/v1.0/aws/AmazonRDS/20260806022930/us-east-1/index.json",
            "/offers/v1.0/aws/AmazonRDS/20260806022930/eu-west-1/index.json?next=1",
        ] {
            assert_eq!(
                aws_region_offer_url(aws, "AmazonRDS", invalid),
                Err(ProviderError::Unsupported)
            );
        }

        let azure = azure_region_scope("swedencentral").expect("Azure scope");
        let retail = azure_retail_url(azure, "USD").expect("Azure Retail URL");
        assert_eq!(retail.host_str(), Some("prices.azure.com"));
        assert_eq!(
            retail
                .query_pairs()
                .find(|(name, _)| name == "$filter")
                .map(|(_, value)| value.into_owned()),
            Some(
                "serviceName eq 'SQL Managed Instance' and armRegionName eq 'swedencentral'"
                    .to_owned()
            )
        );

        let vm_retail = azure_vm_retail_url(azure, "USD").expect("Azure VM Retail URL");
        assert_eq!(
            vm_retail
                .query_pairs()
                .find(|(name, _)| name == "$filter")
                .map(|(_, value)| value.into_owned()),
            Some(
                "serviceName eq 'Virtual Machines' and armRegionName eq 'swedencentral'".to_owned()
            )
        );
        let vm_next = format!("{vm_retail}&$skip=1000");
        assert!(azure_vm_retail_continuation_url(azure, "USD", &vm_next).is_ok());
        assert_eq!(
            azure_managed_disk_retail_continuation_url(azure, "USD", &vm_next),
            Err(ProviderError::Unsupported)
        );

        let disk_retail =
            azure_managed_disk_retail_url(azure, "USD").expect("managed-disk Retail URL");
        assert_eq!(
            disk_retail
                .query_pairs()
                .find(|(name, _)| name == "$filter")
                .map(|(_, value)| value.into_owned()),
            Some(
                "serviceName eq 'Storage' and armRegionName eq 'swedencentral' and (productName eq 'Premium SSD Managed Disks' or productName eq 'Azure Premium SSD v2')"
                    .to_owned()
            )
        );
        let disk_next = format!("{disk_retail}&$skip=1000");
        assert!(azure_managed_disk_retail_continuation_url(azure, "USD", &disk_next).is_ok());
        assert_eq!(
            azure_vm_retail_continuation_url(azure, "USD", &disk_next),
            Err(ProviderError::Unsupported)
        );

        let continuation = azure_retail_continuation_url(
            azure,
            "USD",
            "https://prices.azure.com:443/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27swedencentral%27&$skip=1000",
        )
        .expect("Azure Retail continuation URL");
        assert_eq!(continuation.host_str(), Some("prices.azure.com"));
        assert_eq!(
            continuation
                .query_pairs()
                .find(|(name, _)| name == "$skip")
                .map(|(_, value)| value.into_owned()),
            Some("1000".to_owned())
        );
        for invalid in [
            "https://example.invalid/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27swedencentral%27&$skip=1000",
            "https://prices.azure.com:444/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27swedencentral%27&$skip=1000",
            "https://prices.azure.com/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27eastus%27&$skip=1000",
            "https://prices.azure.com/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27swedencentral%27&$skip=0",
            "https://prices.azure.com/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27swedencentral%27&$skip=32000",
        ] {
            assert_eq!(
                azure_retail_continuation_url(azure, "USD", invalid),
                Err(ProviderError::Unsupported)
            );
        }
    }
}
