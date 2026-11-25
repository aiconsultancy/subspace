//! Snapshot-pinned reads of a single side of a Fleet binary.

use gpui_kit::{AppContext as _, Context};
use subspace_client_fleet::{
    Error, FleetClient,
    diff::{DiffContent, DiffFilePage, DiffSide, DiffTarget},
};
use subspace_framework::dashboard::Toast;

use super::{DiffView, Move, Position};
use crate::diff::view::{DiffEvent, DiffSource, load::validate_page};

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
        binary.read = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let blob = Blob {
                        path: &path,
                        at: &at,
                        side,
                    };
                    blob.read(&client, source.request(mode), &position)
                })
                .await;
            _ = view.update(cx, |view, cx| {
                view.finish_binary(result, destination, direction, cx)
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
                self.fleet.busy = None;
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
    ) -> Result<(DiffContent, bool), Error> {
        let request = DiffFilePage {
            path: self.path,
            at: self.at,
            page: position.cursor.as_deref(),
            side: Some(self.side),
        };
        let page = validate_page(client.diff_file(target, request)?, &request)?;
        let content = page.content.ok_or(Error::InvalidResponse)?;
        validate_content(&content, position, self.side)?;
        Ok((content, page.unstable))
    }
}

fn validate_content(
    content: &DiffContent,
    position: &Position,
    side: DiffSide,
) -> Result<(), Error> {
    if content.side != side
        || content.offset != position.offset
        || content
            .offset
            .checked_add(content.bytes.len() as u64)
            .is_none()
        || (content.next_page.is_some()
            && (content.bytes.is_empty() || content.next_page == position.cursor))
    {
        return Err(Error::InvalidResponse);
    }
    Ok(())
}
