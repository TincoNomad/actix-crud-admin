//! Client color palette, exposed to the templates as CSS variables.

/// Colors used by the admin stylesheet. Every field is a CSS color value.
#[derive(Debug, Clone)]
pub struct Theme {
    pub primary: String,
    pub primary_dark: String,
    pub sidebar_bg: String,
    pub sidebar_text: String,
    pub bg: String,
    pub text: String,
    pub text_light: String,
    pub border: String,
    pub white: String,
    pub error: String,
    pub success: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: "#3b82f6".into(),
            primary_dark: "#2563eb".into(),
            sidebar_bg: "#1e293b".into(),
            sidebar_text: "#f8fafc".into(),
            bg: "#f1f5f9".into(),
            text: "#334155".into(),
            text_light: "#64748b".into(),
            border: "#e2e8f0".into(),
            white: "#ffffff".into(),
            error: "#ef4444".into(),
            success: "#22c55e".into(),
        }
    }
}

impl Theme {
    /// Build a theme from a primary color, deriving the darker shade for hovers.
    pub fn from_primary(primary: &str) -> Self {
        Self {
            primary: primary.to_string(),
            primary_dark: darken(primary),
            ..Self::default()
        }
    }

    /// The `<style>` contents declared inside `:root`.
    pub fn to_css(&self) -> String {
        let vars = [
            ("primary", &self.primary),
            ("primary-dark", &self.primary_dark),
            ("sidebar-bg", &self.sidebar_bg),
            ("sidebar-text", &self.sidebar_text),
            ("bg", &self.bg),
            ("text", &self.text),
            ("text-light", &self.text_light),
            ("border", &self.border),
            ("white", &self.white),
            ("error", &self.error),
            ("success", &self.success),
        ];
        let body: String = vars
            .iter()
            .map(|(name, value)| format!("--{}: {};", name, value))
            .collect();
        format!(":root {{ {} }}", body)
    }
}

/// Newtype so `web::Data<Theme>` cannot collide with another `Data`.
#[derive(Clone)]
pub struct AdminTheme(pub Theme);

/// 20% darker for `#rrggbb`; anything else is returned unchanged.
fn darken(color: &str) -> String {
    let hex = color.trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return color.to_string();
    }
    let channel = |i: usize| {
        let value = u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        (value as f32 * 0.8) as u8
    };
    format!("#{:02x}{:02x}{:02x}", channel(0), channel(2), channel(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_exposes_every_variable() {
        let css = Theme::default().to_css();
        for var in ["--primary:", "--sidebar-bg:", "--border:", "--error:"] {
            assert!(css.contains(var), "missing {}", var);
        }
    }

    #[test]
    fn from_primary_derives_a_darker_shade() {
        let theme = Theme::from_primary("#ff0000");
        assert_eq!(theme.primary, "#ff0000");
        assert_eq!(theme.primary_dark, "#cc0000");
    }

    #[test]
    fn invalid_colors_are_passed_through() {
        let theme = Theme::from_primary("rebeccapurple");
        assert_eq!(theme.primary_dark, "rebeccapurple");
    }
}
