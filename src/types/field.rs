/// Type of input field in the admin form.
#[derive(Debug, Clone, serde::Serialize)]
pub enum FieldType {
    Text,
    TextArea { rows: u8 },
    Number,
    Email,
    Password,
    Boolean,
    Date,
    Select { options: Vec<(String, String)> },
}

impl FieldType {
    fn kind(&self) -> &'static str {
        match self {
            FieldType::Text => "Text",
            FieldType::TextArea { .. } => "TextArea",
            FieldType::Number => "Number",
            FieldType::Email => "Email",
            FieldType::Password => "Password",
            FieldType::Boolean => "Boolean",
            FieldType::Date => "Date",
            FieldType::Select { .. } => "Select",
        }
    }

    fn rows(&self) -> u8 {
        match self {
            FieldType::TextArea { rows } => *rows,
            _ => 0,
        }
    }

    fn options(&self) -> &[(String, String)] {
        match self {
            FieldType::Select { options } => options,
            _ => &[],
        }
    }
}

/// Configuration for a field in the admin create/edit form.
///
/// Serialized flat (`field_type` as a string plus `rows`/`options`) because the
/// Tera templates compare `field.field_type` against strings.
#[derive(Debug, Clone)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub field_type: FieldType,
    pub required: bool,
    pub placeholder: Option<String>,
    pub help_text: Option<String>,
    pub fieldset: Option<String>,
}

impl serde::Serialize for FormField {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("FormField", 9)?;
        st.serialize_field("key", &self.key)?;
        st.serialize_field("label", &self.label)?;
        st.serialize_field("field_type", self.field_type.kind())?;
        st.serialize_field("required", &self.required)?;
        st.serialize_field("placeholder", &self.placeholder)?;
        st.serialize_field("help_text", &self.help_text)?;
        st.serialize_field("fieldset", &self.fieldset)?;
        st.serialize_field("rows", &self.field_type.rows())?;
        st.serialize_field("options", self.field_type.options())?;
        st.end()
    }
}

impl FormField {
    pub fn text(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::Text,
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn number(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::Number,
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn email(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::Email,
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn password(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::Password,
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn textarea(key: &str, label: &str, rows: u8) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::TextArea { rows },
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn boolean(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::Boolean,
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn date(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::Date,
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn select(key: &str, label: &str, options: Vec<(&str, &str)>) -> Self {
        let options = options
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Self {
            key: key.to_string(),
            label: label.to_string(),
            field_type: FieldType::Select { options },
            required: false,
            placeholder: None,
            help_text: None,
            fieldset: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn placeholder(mut self, p: &str) -> Self {
        self.placeholder = Some(p.to_string());
        self
    }

    pub fn help(mut self, h: &str) -> Self {
        self.help_text = Some(h.to_string());
        self
    }

    pub fn fieldset(mut self, name: &str) -> Self {
        self.fieldset = Some(name.to_string());
        self
    }
}
