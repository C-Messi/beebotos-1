# BeeBotOS Context

This glossary keeps BeeBotOS product language precise across user-facing workflows and internal planning.

## Language

**移出队列**:
Forget a video generation task from the user's local queue record without changing the remote generation task or the generated video.
_Avoid_: 删除队列, 删除任务, 取消任务

**取消生成**:
Request that the remote video provider stops a queued or running video generation task.
_Avoid_: 删除队列, 移出队列, 删除本地视频

**删除本地视频**:
Remove the local playable video for a completed video generation task while leaving the task and remote generation result unchanged.
_Avoid_: 删除视频, 删除远端视频, 取消任务

**本地视频已删除**:
A user-chosen state where a completed video task remains in the queue, but its local playable video is intentionally absent until the user restores it.
_Avoid_: 视频失败, 远端视频已删除, 任务已删除

**恢复视频**:
Recreate the local playable video for a completed video generation task whose local video was intentionally deleted.
_Avoid_: 重新生成视频, 恢复队列, 取消删除

**恢复任务**:
Recreate a user's local queue record for a remote video generation task by its task ID.
_Avoid_: 恢复视频, 重新生成视频, 导入全局任务
