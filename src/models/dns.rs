use serde::{Deserialize, Serialize};
use strum::{AsRefStr, VariantArray};

#[derive(Clone, Serialize)]
pub struct DnsQueryRequest {
    pub name: String,
    pub r#type: DnsRecordType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsQueryResponse {
    #[serde(default, rename = "Answer")]
    pub answer: Vec<DnsAnswer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsAnswer {
    pub name: String,
    pub data: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, AsRefStr, VariantArray, Serialize)]
#[strum(serialize_all = "UPPERCASE")]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsRecordType {
    A,
    Aaaa,
    Cname,
    Txt,
    Mx,
    Srv,
    Https,
    Ns,
    Dnskey,
    Ds,
    Sig,
    Soa,
    Rrsig,
    Rp,
}
