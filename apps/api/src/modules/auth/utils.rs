use rand::RngExt;

pub fn generate_otp() -> String {
    let code: u32 = rand::rng().random_range(100_000..=999_999);
    format!("{:06}", code)
}
