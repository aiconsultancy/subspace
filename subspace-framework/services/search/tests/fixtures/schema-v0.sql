-- SQLite schema shipped in Subspace 1.4.0 (unversioned).
CREATE TABLE settings(id INTEGER PRIMARY KEY CHECK(id=1), policy TEXT NOT NULL, active TEXT, generation INTEGER NOT NULL);
CREATE TABLE authority(id INTEGER PRIMARY KEY CHECK(id=1), epoch INTEGER NOT NULL, sequence INTEGER NOT NULL);
INSERT INTO authority VALUES(1,0,0);
CREATE TABLE document_versions(scope TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,PRIMARY KEY(scope,id));
CREATE TABLE documents(rowid INTEGER PRIMARY KEY, scope TEXT NOT NULL, id TEXT NOT NULL, refreshed INTEGER NOT NULL, json TEXT NOT NULL, UNIQUE(scope,id));
CREATE INDEX documents_expiry ON documents(refreshed);
CREATE INDEX documents_scope_expiry ON documents(scope,refreshed);
CREATE VIRTUAL TABLE text_index USING fts5(title,body,metadata, tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER delete_document AFTER DELETE ON documents BEGIN DELETE FROM text_index WHERE rowid=old.rowid; END;
CREATE TRIGGER delete_version AFTER DELETE ON documents BEGIN DELETE FROM document_versions WHERE scope=old.scope AND id=old.id; END;
