-- Where the administrator role came from that let an invite's issuer invite another person, which
-- redemption checks again. Null for older rows: redemption then checks the provisioned role.
ALTER TABLE "pairing_invites" ADD COLUMN "issuer_authority" text;
