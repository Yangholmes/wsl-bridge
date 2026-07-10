# Safety

Require user confirmation for:

- Writing the system hosts file.
- Activating a hosts group.
- Deleting Proxy objects.
- Binding a listener to `0.0.0.0`.
- Installing or overwriting Agent skill files.
- Importing configuration that overwrites existing objects.

Prefer read and dry-run operations by default.
