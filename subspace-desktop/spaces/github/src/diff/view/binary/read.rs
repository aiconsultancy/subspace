//! Snapshot-pinned reads of a single side of a Fleet binary.

use gpui_kit::{AppContext as _, Context};
use subspace_client_fleet::{
    Error, FleetClient,
    diff::{DiffContent, DiffFilePage, DiffRead, DiffSide, DiffTarget},
};
use subspace_framework::dashboard::Toast;

use super::{DiffView, Move, Position, Retry};
use crate::diff::view::{DiffEvent, DiffSource, pages::validate_page};

impl DiffView {
    pub(super) fn read_binary(&mut self, direction: Move, cx: &mut Context<Self>) {
        let (DiffSource::Fleet(source), Some(at), Some(binary)) = (
            self.source.clone(),
            self.fleet.fingerprint.clone(),
            self.binary.as_mut(),
        ) else {
            return;
        };
        if self.fleet.changed() || binary.read.is_some() {
            return;
        }
        let Some(position) = binary.destination(&direction) else {
            return;
        };
        let client = match source.client(cx) {
            Ok(client) => client,
            Err(help) => {
                cx.emit(DiffEvent::Toast(Toast::error(help)));
                return;
            }
        };
        let path = self.files[binary.file].file.path.clone();
        let (side, mode) = (binary.side, self.fleet.mode);
        let destination = position.clone();
        let closed = binary.closed.clone();
        let guard = closed.clone();
        binary.retry = None;
        binary.read = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let blob = Blob {
                        path: &path,
                        at: &at,
                        side,
                    };
                    blob.read(&client, source.request(mode), &position, &closed)
                })
                .await;
            _ = view.update(cx, |view, cx| {
                if guard.check().is_ok() {
                    view.finish_binary(result, destination, direction, cx);
                }
            });
        }));
        cx.notify();
    }

    fn finish_binary(
        &mut self,
        result: Result<(DiffContent, bool), Error>,
        position: Position,
        direction: Move,
        cx: &mut Context<Self>,
    ) {
        let Some(binary) = self.binary.as_mut() else {
            return;
        };
        binary.read = None;
        match result {
            Ok((content, unstable)) => {
                binary.show(content, position, direction);
                self.fleet.unstable |= unstable;
            }
            Err(Error::Busy(wait)) => {
                binary.retry = Some((Retry::Bytes(direction), wait));
            }
            Err(error) => self.fleet_page_error(error, cx),
        }
        cx.notify();
    }
}

pub(super) struct Blob<'a> {
    pub(super) path: &'a str,
    pub(super) at: &'a str,
    pub(super) side: DiffSide,
}

impl Blob<'_> {
    pub(super) fn read(
        &self,
        client: &FleetClient,
        target: DiffTarget<'_>,
        position: &Position,
        read: &DiffRead,
    ) -> Result<(DiffContent, bool), Error> {
        let request = DiffFilePage {
            path: self.path,
            at: self.at,
            page: position.cursor.as_deref(),
            side: Some(self.side),
        };
        let page = validate_page(read.retry(|| client.diff_file(target, request))?, &request)?;
        let mut content = page.content.ok_or(Error::InvalidResponse)?;
        let end = validate_content(&content, position, self.side)?;
        clear_eof_cursor(&mut content, end);
        Ok((content, page.unstable))
    }
}

fn validate_content(
    content: &DiffContent,
    position: &Position,
    side: DiffSide,
) -> Result<u64, Error> {
    let end = content
        .offset
        .checked_add(content.bytes.len() as u64)
        .ok_or(Error::InvalidResponse)?;
    if content.side != side
        || content.offset != position.offset
        || end > content.size
        || match &content.next_page {
            Some(_) => {
                (end < content.size && content.bytes.is_empty())
                    || content.next_page == position.cursor
            }
            None => end != content.size,
        }
    {
        return Err(Error::InvalidResponse);
    }
    Ok(end)
}

fn clear_eof_cursor(content: &mut DiffContent, end: u64) {
    // Older gateways may offer an empty terminator after the last bytes.
    // The verified size already tells both the pager and the preview to stop.
    if end == content.size {
        content.next_page = None;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DiffContent, DiffSide, Error, Position, clear_eof_cursor, validate_content};

    #[test]
    fn rejects_a_short_final_page_and_an_empty_continuation() {
        let position = Position {
            cursor: Some("last".into()),
            offset: 2,
        };
        for (size, data, next, expected) in [
            (4, "QQ==", None, Err(Error::InvalidResponse)),
            (3, "", None, Err(Error::InvalidResponse)),
            (3, "", Some("no-progress"), Err(Error::InvalidResponse)),
            (2, "QQ==", None, Err(Error::InvalidResponse)),
            (3, "QQ==", None, Ok(3)),
        ] {
            let page: DiffContent = serde_json::from_value(json!({
                "side": "new", "encoding": "base64", "offset": 2, "size": size, "data": data,
                "nextPage": next,
            }))
            .unwrap();
            assert_eq!(validate_content(&page, &position, DiffSide::New), expected);
        }
    }

    #[test]
    fn validates_empty_files_and_exact_mib_boundaries() {
        const MIB: usize = 1024 * 1024;
        let data = "AAAA".repeat(MIB / 3) + "AA==";
        for (offset, size, cursor, data, next) in [
            (0, 0, None, "", None),
            (0, 2 * MIB, None, data.as_str(), Some("second")),
            (MIB, 2 * MIB, Some("second"), data.as_str(), None),
            (MIB, 2 * MIB, Some("second"), data.as_str(), Some("end")),
            (2 * MIB, 2 * MIB, Some("end"), "", None),
        ] {
            let position = Position {
                cursor: cursor.map(str::to_owned),
                offset: offset as u64,
            };
            let mut page: DiffContent = serde_json::from_value(json!({
                "side": "new", "encoding": "base64", "offset": offset, "size": size,
                "data": data, "nextPage": next,
            }))
            .unwrap();
            assert_eq!(page.bytes.len(), if data.is_empty() { 0 } else { MIB });
            let end = validate_content(&page, &position, DiffSide::New).unwrap();
            assert_eq!(end, (offset + page.bytes.len()) as u64);
            assert_eq!(page.next_page.as_deref(), next);
            clear_eof_cursor(&mut page, end);
            assert_eq!(
                page.next_page.as_deref(),
                if end < page.size { next } else { None }
            );
        }
    }
}
