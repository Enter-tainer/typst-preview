use crate::DocToSrcJumpInfo;

pub enum EditorActorRequest {
    DocToSrcJump(DocToSrcJumpInfo),
}
