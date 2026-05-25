pub type AsyncJobSequence = u64;

pub fn advance_async_job_sequence(sequence: &mut AsyncJobSequence) -> AsyncJobSequence {
    *sequence = sequence.saturating_add(1);
    *sequence
}

#[derive(Debug, Clone, PartialEq)]
pub struct SequencedAsyncCache<Status, Output, Error> {
    sequence: AsyncJobSequence,
    status: Status,
    output: Option<Output>,
    error: Option<Error>,
}

impl<Status, Output, Error> SequencedAsyncCache<Status, Output, Error>
where
    Status: Copy,
{
    pub const fn new(status: Status) -> Self {
        Self {
            sequence: 0,
            status,
            output: None,
            error: None,
        }
    }

    pub const fn sequence(&self) -> AsyncJobSequence {
        self.sequence
    }

    pub const fn status(&self) -> Status {
        self.status
    }

    pub fn output(&self) -> Option<&Output> {
        self.output.as_ref()
    }

    pub fn output_mut(&mut self) -> Option<&mut Output> {
        self.output.as_mut()
    }

    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }

    pub fn mark_empty(&mut self, sequence: AsyncJobSequence, status: Status) {
        self.sequence = sequence;
        self.status = status;
        self.output = None;
        self.error = None;
    }

    pub fn mark_pending(&mut self, sequence: AsyncJobSequence, status: Status) {
        self.sequence = sequence;
        self.status = status;
        self.error = None;
    }

    pub fn publish_result(
        &mut self,
        sequence: AsyncJobSequence,
        result: Result<Output, Error>,
        ready_status: Status,
        error_status: Status,
    ) -> bool {
        if sequence != self.sequence {
            return false;
        }

        match result {
            Ok(output) => {
                self.output = Some(output);
                self.error = None;
                self.status = ready_status;
            }
            Err(error) => {
                self.output = None;
                self.error = Some(error);
                self.status = error_status;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Status {
        Idle,
        Running,
        Ready,
        Error,
    }

    #[test]
    fn sequenced_cache_rejects_stale_results() {
        let mut cache = SequencedAsyncCache::new(Status::Idle);
        cache.mark_empty(2, Status::Running);

        assert!(!cache.publish_result(
            1,
            Ok::<_, &'static str>("old"),
            Status::Ready,
            Status::Error
        ));
        assert_eq!(cache.status(), Status::Running);
        assert_eq!(cache.output(), None);
    }

    #[test]
    fn sequenced_cache_publishes_matching_result() {
        let mut cache = SequencedAsyncCache::new(Status::Idle);
        let mut sequence = 0;
        let sequence = advance_async_job_sequence(&mut sequence);
        cache.mark_empty(sequence, Status::Running);

        assert!(cache.publish_result(
            sequence,
            Ok::<_, &'static str>("current"),
            Status::Ready,
            Status::Error
        ));
        assert_eq!(cache.status(), Status::Ready);
        assert_eq!(cache.output(), Some(&"current"));
    }
}
