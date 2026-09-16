pub fn is_ground(net: &str) -> bool {
    let upper = net.trim().to_ascii_uppercase();
    upper == "GND"
        || upper.starts_with("GND_")
        || upper.starts_with("GND-")
        || upper.ends_with("_GND")
        || upper.ends_with("-GND")
}

pub fn is_power(net: &str) -> bool {
    let value = net.trim().to_ascii_uppercase();
    if is_ground(&value) {
        return false;
    }
    matches!(
        value.as_str(),
        "VBUS"
            | "VCC"
            | "VDD"
            | "VIN"
            | "VOUT"
            | "VSYS"
            | "VBAT"
            | "BAT"
            | "BAT+"
            | "BATT"
            | "BATT+"
            | "AVDD"
            | "DVDD"
            | "IOVDD"
            | "ADC_AVDD"
            | "VREF"
    ) || value.starts_with('+') && value.contains('V')
        || value.chars().next().is_some_and(|c| c.is_ascii_digit()) && value.contains('V')
        || ["VCC_", "VDD_", "VIN_", "VOUT_", "VBAT_", "VSYS_"]
            .iter()
            .any(|prefix| value.starts_with(prefix))
}

pub fn is_switching_power(net: &str) -> bool {
    let value = net.trim().to_ascii_uppercase();
    ["SW", "LX", "PH", "BOOT", "BST", "SWNODE", "VREG_LX"]
        .iter()
        .any(|prefix| {
            value == *prefix
                || value.starts_with(&format!("{prefix}_"))
                || value.starts_with(&format!("{prefix}-"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_existing_power_names() {
        assert!(is_ground("GND_A"));
        assert!(is_power("3V3"));
        assert!(is_power("VDD_IO"));
        assert!(is_switching_power("SW_NODE"));
        assert!(!is_power("GPIO3"));
    }
}
