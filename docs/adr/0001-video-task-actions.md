# Separate Video Task Actions

AI video marketing uses distinct actions for queue cleanup, remote cancellation, local video deletion, and local video restoration. `DELETE /video-tasks/:id` means forgetting the user's local queue record, while remote cancellation uses an explicit cancel action; this avoids treating local cleanup as provider-side task control.
