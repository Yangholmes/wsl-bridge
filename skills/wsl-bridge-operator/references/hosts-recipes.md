# Hosts Recipes

## Create and Activate a Hosts Group

1. Inspect `wsl-bridge://state/hosts`.
2. Build a ConfigPatch that creates the group and records.
3. Dry-run to detect invalid IPs, duplicate domains, and permission requirements.
4. Ask for confirmation if activation will write the system hosts file.
5. Apply and validate the active group.

## Import Hosts

Use import dry-run first. Report create, update, duplicate, and invalid-record counts before applying.
