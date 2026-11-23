//! Excel serialization and deserialization for Witcher3StringEditor.
//! Uses calamine for reading and rust_xlsxwriter for writing.
#![forbid(unsafe_code)]

use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};
use rust_xlsxwriter::{
    Color, Format, FormatAlign, FormatBorder, Workbook,
};
pub use w3codec_rs::port::W3StringItem;

/// Deserializes The Witcher 3 string items from an Excel workbook (.xlsx).
///
/// Follows the C# original:
/// - Reads the first worksheet.
/// - Row 0 is the table header (StrId, KeyHex, KeyName, OldText, Text).
/// - OldText is intentionally reset to empty string (baseline text is session-only).
pub fn read_xlsx(path: &Path) -> Result<Vec<W3StringItem>, String> {
    let mut workbook = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let sheet_name = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| "No worksheets found in workbook".to_string())?;

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| e.to_string())?;

    let mut items = Vec::new();

    let cell_to_string = |val: Option<&Data>| -> String {
        match val {
            Some(Data::String(s)) => s.clone(),
            Some(Data::Int(i)) => i.to_string(),
            Some(Data::Float(f)) => {
                if f.fract() == 0.0 {
                    (*f as i64).to_string()
                } else {
                    f.to_string()
                }
            }
            Some(Data::Bool(b)) => b.to_string(),
            Some(Data::DateTime(d)) => d.to_string(),
            Some(Data::DateTimeIso(d)) => d.clone(),
            Some(Data::DurationIso(d)) => d.clone(),
            Some(Data::Error(_)) | Some(Data::Empty) | None => String::new(),
        }
    };

    let mut rows_iter = range.rows();
    // Skip header row
    let _ = rows_iter.next();

    for row in rows_iter {
        let str_id = cell_to_string(row.first());
        let key_hex = cell_to_string(row.get(1));
        let key_name = cell_to_string(row.get(2));
        // row.get(3) is OldText, which the C# original sets to string.Empty
        let text = cell_to_string(row.get(4));

        if str_id.is_empty() && key_hex.is_empty() && key_name.is_empty() && text.is_empty() {
            continue;
        }

        items.push(W3StringItem {
            str_id,
            key_hex,
            key_name,
            old_text: String::new(),
            text,
        });
    }

    Ok(items)
}

/// Serializes The Witcher 3 string items to an Excel workbook (.xlsx).
///
/// Follows the formatting of Syncfusion.XlsIO in C#:
/// - Table headers: StrId, KeyHex, KeyName, OldText, Text.
/// - Header style: Bold, White font, Gray background (80% gray), Center-aligned.
/// - Data columns A-C: Center-aligned.
/// - Data columns D-E: Text-wrapped.
/// - All cells: Thin border, text format ("@").
/// - Freeze row 1.
/// - Column widths: A-B = 15, C = 30, D-E = 50.
pub fn write_xlsx(path: &Path, items: &[W3StringItem]) -> Result<(), String> {
    if items.is_empty() {
        return Err("Cannot serialize empty string items list".to_string());
    }

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    // Formats
    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::RGB(0xFFFFFF))
        .set_background_color(Color::RGB(0x404040))
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_num_format("@");

    let centered_data_format = Format::new()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_num_format("@");

    let text_data_format = Format::new()
        .set_text_wrap()
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_num_format("@");

    // Headers
    let headers = ["StrId", "KeyHex", "KeyName", "OldText", "Text"];
    for (col_idx, &h) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, col_idx as u16, h, &header_format)
            .map_err(|e| e.to_string())?;
    }

    // Set Column Widths: A-B (15), C (30), D-E (50)
    worksheet.set_column_width(0, 15.0).map_err(|e| e.to_string())?;
    worksheet.set_column_width(1, 15.0).map_err(|e| e.to_string())?;
    worksheet.set_column_width(2, 30.0).map_err(|e| e.to_string())?;
    worksheet.set_column_width(3, 50.0).map_err(|e| e.to_string())?;
    worksheet.set_column_width(4, 50.0).map_err(|e| e.to_string())?;

    // Freeze header row
    worksheet.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;

    // Data rows
    for (row_idx, item) in items.iter().enumerate() {
        let r = (row_idx + 1) as u32;
        worksheet
            .write_string_with_format(r, 0, &item.str_id, &centered_data_format)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string_with_format(r, 1, &item.key_hex, &centered_data_format)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string_with_format(r, 2, &item.key_name, &centered_data_format)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string_with_format(r, 3, &item.old_text, &text_data_format)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string_with_format(r, 4, &item.text, &text_data_format)
            .map_err(|e| e.to_string())?;
    }

    workbook.save(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xlsx_round_trip() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_w3strings.xlsx");

        let items = vec![
            W3StringItem {
                str_id: "1001".into(),
                key_hex: "0x12345678".into(),
                key_name: "test_key".into(),
                old_text: "Old value".into(),
                text: "New text value".into(),
            },
            W3StringItem {
                str_id: "1002".into(),
                key_hex: "0xABCDEF00".into(),
                key_name: "second_key".into(),
                old_text: "".into(),
                text: "Second text".into(),
            },
        ];

        write_xlsx(&path, &items).unwrap();
        assert!(path.exists());

        let read_items = read_xlsx(&path).unwrap();
        assert_eq!(read_items.len(), 2);
        assert_eq!(read_items[0].str_id, "1001");
        assert_eq!(read_items[0].key_hex, "0x12345678");
        assert_eq!(read_items[0].key_name, "test_key");
        assert_eq!(read_items[0].old_text, ""); // OldText intentionally reset per C# spec
        assert_eq!(read_items[0].text, "New text value");

        assert_eq!(read_items[1].str_id, "1002");
        assert_eq!(read_items[1].text, "Second text");

        let _ = std::fs::remove_file(path);
    }
}
