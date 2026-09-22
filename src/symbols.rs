use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowSymbols {
    pub whitespace: bool,
    pub eol: bool,
    pub non_printing: bool,
    pub controls: bool,
    pub indent_guides: bool,
    pub wrap_markers: bool,
}

impl Default for ShowSymbols {
    fn default() -> Self {
        Self {
            whitespace: false,
            eol: false,
            non_printing: false,
            // Preserve Scintilla's existing control-character visibility for older sessions.
            controls: true,
            indent_guides: false,
            wrap_markers: false,
        }
    }
}

impl ShowSymbols {
    pub fn all_characters(self) -> bool {
        self.whitespace && self.eol && self.non_printing && self.controls
    }
    pub fn toggle_all(&mut self) {
        let enabled = !self.all_characters();
        self.whitespace = enabled;
        self.eol = enabled;
        self.non_printing = enabled;
        self.controls = enabled;
    }
}

pub const NON_PRINTING: &[(char, &str)] = &[
    ('\u{a0}', "NBSP"),
    ('\u{ad}', "SHY"),
    ('\u{61c}', "ALM"),
    ('\u{1680}', "OSPM"),
    ('\u{180e}', "MVS"),
    ('\u{2000}', "NQSP"),
    ('\u{2001}', "MQSP"),
    ('\u{2002}', "ENSP"),
    ('\u{2003}', "EMSP"),
    ('\u{2004}', "3/MSP"),
    ('\u{2005}', "4/MSP"),
    ('\u{2006}', "6/MSP"),
    ('\u{2007}', "FSP"),
    ('\u{2008}', "PSP"),
    ('\u{2009}', "THSP"),
    ('\u{200a}', "HSP"),
    ('\u{200b}', "ZWSP"),
    ('\u{200c}', "ZWNJ"),
    ('\u{200d}', "ZWJ"),
    ('\u{200e}', "LRM"),
    ('\u{200f}', "RLM"),
    ('\u{202a}', "LRE"),
    ('\u{202b}', "RLE"),
    ('\u{202c}', "PDF"),
    ('\u{202d}', "LRO"),
    ('\u{202e}', "RLO"),
    ('\u{202f}', "NNBSP"),
    ('\u{205f}', "MMSP"),
    ('\u{2060}', "WJ"),
    ('\u{2066}', "LRI"),
    ('\u{2067}', "RLI"),
    ('\u{2068}', "FSI"),
    ('\u{2069}', "PDI"),
    ('\u{206a}', "ISS"),
    ('\u{206b}', "ASS"),
    ('\u{206c}', "IAFS"),
    ('\u{206d}', "AAFS"),
    ('\u{206e}', "NADS"),
    ('\u{206f}', "NODS"),
    ('\u{3000}', "IDSP"),
    ('\u{feff}', "ZWNBSP"),
];

pub const C0: &[&str; 32] = &[
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
    "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC",
    "FS", "GS", "RS", "US",
];
pub const C1: &[&str; 32] = &[
    "PAD", "HOP", "BPH", "NBH", "IND", "NEL", "SSA", "ESA", "HTS", "HTJ", "VTS", "PLD", "PLU",
    "RI", "SS2", "SS3", "DCS", "PU1", "PU2", "STS", "CCH", "MW", "SPA", "EPA", "SOS", "SGCI",
    "SCI", "CSI", "ST", "OSC", "PM", "APC",
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_characters_does_not_change_guides() {
        let mut view = ShowSymbols {
            indent_guides: true,
            ..Default::default()
        };
        assert!(!view.all_characters());
        view.toggle_all();
        assert!(view.all_characters());
        view.eol = false;
        assert!(!view.all_characters());
        view.toggle_all();
        assert!(view.all_characters());
        view.toggle_all();
        assert!(!view.whitespace && !view.eol && !view.controls && !view.non_printing);
        assert!(view.indent_guides);
        assert!(!view.wrap_markers);
    }
    #[test]
    fn preferences_round_trip_and_old_sessions_keep_defaults() {
        assert_eq!(
            serde_json::from_str::<ShowSymbols>("{}").unwrap(),
            ShowSymbols::default()
        );
        let mut view = ShowSymbols::default();
        view.toggle_all();
        assert_eq!(
            serde_json::from_str::<ShowSymbols>(&serde_json::to_string(&view).unwrap()).unwrap(),
            view
        );
    }
}
