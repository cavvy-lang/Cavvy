fn main() {
    println!("cargo:rerun-if-changed=../.verinfo");
    let verinfo = std::fs::read_to_string("../.verinfo")
        .expect("cay-setup must be built from the Cavvy workspace with .verinfo");
    let setup_version = section_value(&verinfo, "CAY-SETUP", "version")
        .expect(".verinfo is missing CAY-SETUP.version");
    println!("cargo:rustc-env=CAY_SETUP_VERSION={setup_version}");

    #[cfg(windows)]
    {
        winresource::WindowsResource::new()
            .set_manifest_file("cay-setup.manifest")
            .compile()
            .expect("failed to embed cay-setup Windows manifest");
    }
}

fn section_value<'a>(content: &'a str, wanted_section: &str, wanted_key: &str) -> Option<&'a str> {
    let mut section = "";
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
        } else if section == wanted_section
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == wanted_key
        {
            return Some(value.trim().trim_matches('"'));
        }
    }
    None
}
