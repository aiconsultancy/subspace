-- Only the two authorities an invite can record, or null. Every existing row was written by the
-- API with one of them or with none, so it already satisfies the check.
ALTER TABLE "pairing_invites" ADD CONSTRAINT "pairing_invites_issuer_authority" CHECK ("issuer_authority" in ('entraAppRole', 'provisioned'));
