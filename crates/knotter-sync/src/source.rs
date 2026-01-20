use crate::Result;

pub trait VcfSource {
    fn source_name(&self) -> &'static str;
    fn fetch_vcf(&self) -> Result<String>;
}
