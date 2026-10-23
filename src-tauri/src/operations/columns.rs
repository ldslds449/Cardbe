//! Validated column mutations shared by plugin changesets and registry operations.
use crate::{board_mutations as mutation, errors::DomainError, models::StoredData};

pub(crate) fn valid_column(name: &str, color: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 4096 && color.len() <= 64
}

/// Appends a custom-ordered column.
pub(crate) fn create_column(
    data: &mut StoredData,
    name: String,
    color: String,
) -> Result<i64, DomainError> {
    if !valid_column(&name, &color) {
        return Err(DomainError::InvalidArgument);
    }
    mutation::add_column(data, name, color)
}

/// Renames and recolors a column, returning its index.
pub(crate) fn update_column(
    data: &mut StoredData,
    column_id: i64,
    name: String,
    color: String,
) -> Result<usize, DomainError> {
    if !valid_column(&name, &color) {
        return Err(DomainError::InvalidArgument);
    }
    let index = mutation::column_index(data, column_id)?;
    let column = &mut data.columns[index];
    column.name = name;
    column.color = color;
    Ok(index)
}

/// Moves a column before another one, or to the end.
pub(crate) fn move_column(
    data: &mut StoredData,
    column_id: i64,
    before_column_id: Option<i64>,
) -> Result<(), DomainError> {
    mutation::column_index(data, column_id)?;
    mutation::move_column(data, column_id, before_column_id)
}
