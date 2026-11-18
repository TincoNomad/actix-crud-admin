//! UI strings. English by default; use [`Labels::spanish`] for Spanish.

use serde::Serialize;

/// Every label the templates render. Kept as a struct so a missing
/// translation is a compile error, not an empty string.
#[derive(Debug, Clone, Serialize)]
pub struct Labels {
    /// BCP-47 language tag, used by `<html lang>`.
    pub locale: &'static str,
    pub dashboard: &'static str,
    pub home: &'static str,
    pub logout: &'static str,
    pub logged_in_as: &'static str,
    pub login: &'static str,
    pub username: &'static str,
    pub password: &'static str,
    pub invalid_login: &'static str,
    pub add: &'static str,
    pub change: &'static str,
    pub save: &'static str,
    pub save_continue: &'static str,
    pub save_another: &'static str,
    pub cancel: &'static str,
    pub delete: &'static str,
    pub edit: &'static str,
    pub search: &'static str,
    pub no_records: &'static str,
    pub actions: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
    pub confirm_delete: &'static str,
    pub prev: &'static str,
    pub next: &'static str,
    pub page: &'static str,
    pub of: &'static str,
    pub required: &'static str,
    pub filters: &'static str,
    pub all: &'static str,
    pub apply: &'static str,
    pub bulk_delete: &'static str,
    pub export_csv: &'static str,
    pub change_password: &'static str,
    pub current_password: &'static str,
    pub new_password: &'static str,
    pub confirm_password: &'static str,
    pub wrong_password: &'static str,
    pub password_mismatch: &'static str,
    pub password_changed: &'static str,
    pub audit: &'static str,
    pub audit_user: &'static str,
    pub audit_action: &'static str,
    pub audit_resource: &'static str,
    pub audit_record: &'static str,
    pub audit_when: &'static str,
    pub no_audit: &'static str,
    pub forbidden: &'static str,
    pub csrf_error: &'static str,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            locale: "en",
            dashboard: "Dashboard",
            home: "Home",
            logout: "Logout",
            logged_in_as: "Logged in as",
            login: "Log in",
            username: "Username",
            password: "Password",
            invalid_login: "Invalid username or password",
            add: "Add",
            change: "Change",
            save: "Save",
            save_continue: "Save and continue editing",
            save_another: "Save and add another",
            cancel: "Cancel",
            delete: "Delete",
            edit: "Edit",
            search: "Search",
            no_records: "No records found.",
            actions: "Actions",
            yes: "Yes",
            no: "No",
            confirm_delete: "Are you sure?",
            prev: "Previous",
            next: "Next",
            page: "Page",
            of: "of",
            required: "This field is required.",
            filters: "Filters",
            all: "All",
            apply: "Apply",
            bulk_delete: "Delete selected",
            export_csv: "Export CSV",
            change_password: "Change password",
            current_password: "Current password",
            new_password: "New password",
            confirm_password: "Confirm new password",
            wrong_password: "Current password is incorrect",
            password_mismatch: "The new passwords do not match",
            password_changed: "Password updated",
            audit: "Audit log",
            audit_user: "User",
            audit_action: "Action",
            audit_resource: "Resource",
            audit_record: "Record",
            audit_when: "When",
            no_audit: "No activity recorded.",
            forbidden: "You are not allowed to perform this action",
            csrf_error: "Invalid or missing security token. Reload the page and try again.",
        }
    }
}

impl Labels {
    pub fn english() -> Self {
        Self::default()
    }

    pub fn spanish() -> Self {
        Self {
            locale: "es",
            dashboard: "Panel",
            home: "Inicio",
            logout: "Salir",
            logged_in_as: "Sesión de",
            login: "Entrar",
            username: "Usuario",
            password: "Contraseña",
            invalid_login: "Usuario o contraseña incorrectos",
            add: "Agregar",
            change: "Editar",
            save: "Guardar",
            save_continue: "Guardar y seguir editando",
            save_another: "Guardar y agregar otro",
            cancel: "Cancelar",
            delete: "Eliminar",
            edit: "Editar",
            search: "Buscar",
            no_records: "No hay registros.",
            actions: "Acciones",
            yes: "Sí",
            no: "No",
            confirm_delete: "¿Estás seguro?",
            prev: "Anterior",
            next: "Siguiente",
            page: "Página",
            of: "de",
            required: "Este campo es obligatorio.",
            filters: "Filtros",
            all: "Todos",
            apply: "Aplicar",
            bulk_delete: "Eliminar seleccionados",
            export_csv: "Exportar CSV",
            change_password: "Cambiar contraseña",
            current_password: "Contraseña actual",
            new_password: "Nueva contraseña",
            confirm_password: "Confirmar nueva contraseña",
            wrong_password: "La contraseña actual es incorrecta",
            password_mismatch: "Las contraseñas nuevas no coinciden",
            password_changed: "Contraseña actualizada",
            audit: "Registro de actividad",
            audit_user: "Usuario",
            audit_action: "Acción",
            audit_resource: "Recurso",
            audit_record: "Registro",
            audit_when: "Cuándo",
            no_audit: "Sin actividad registrada.",
            forbidden: "No tienes permiso para esta acción",
            csrf_error: "Token de seguridad inválido. Recarga la página e intenta de nuevo.",
        }
    }
}

/// Newtype so `web::Data<Labels>` cannot collide with another `Data`.
#[derive(Clone)]
pub struct AdminLabels(pub Labels);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default() {
        assert_eq!(Labels::default().dashboard, "Dashboard");
        assert_eq!(Labels::default().save, "Save");
    }

    #[test]
    fn spanish_translates_every_visible_string() {
        let es = Labels::spanish();
        assert_eq!(es.dashboard, "Panel");
        assert_eq!(es.save, "Guardar");
        assert_ne!(es.search, Labels::default().search);
    }

    #[test]
    fn labels_serialize_for_tera() {
        let value = serde_json::to_value(Labels::spanish()).unwrap();
        assert_eq!(value["delete"], serde_json::json!("Eliminar"));
        assert_eq!(value["actions"], serde_json::json!("Acciones"));
    }
}
