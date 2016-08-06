use std::sync::mpsc::{channel, sync_channel, Sender, SyncSender, Receiver};

use server::ThreadEnv;

pub fn new<'a, E: ThreadEnv<'a>>(size: usize, env: E) -> (Vec<E::Handle>, Worker<'a>) {
    let mut handles = Vec::with_capacity(size + 1);
    let (worker_send, worker_recv) = channel();
    let (work_send, work_recv) = channel();

    for _ in 0..size {
        let worker_send = worker_send.clone();
        handles.push(env.spawn(move || {
            let (send, recv) = channel();
            while worker_send.send(send.clone()).is_ok() {
                if let Ok(work) = recv.recv() {
                    match work {
                        WorkerMessage::Task(work) => work.call_box(),
                        WorkerMessage::End => break,
                    }
                }
            }
        }));
    }

    handles.push(env.spawn(move || {
        while let Ok(work) = work_recv.recv() {
            if let Ok(worker) = worker_recv.recv() {
                let _ = worker.send(WorkerMessage::Task(work));
            }
        }
        while let Ok(worker) = worker_recv.recv() {
            let _ = worker.send(WorkerMessage::End);
        }
    }));

    (handles, Worker(work_send))
}

///A worker pool for somewhat short lived tasks.
///
///Worker threads are meant for tasks that may block, but preferably not for
///very long. Possible use cases are file IO and database queries. The number
///of workers are limited, so it's better to spawn a thread if a task may
///block for too long.
#[derive(Clone)]
pub struct Worker<'a>(Sender<Box<FnBox + Send + 'a>>);

impl<'a> Worker<'a> {
    ///Submit a new task to the workers.
    pub fn new_task<F: FnOnce() + Send + 'a>(&self, f: F) {
        let _ = self.0.send(Box::new(f));
    }

    ///Stream data from a task.
    pub fn stream<F: FnOnce(Sender<T>) + Send + 'a, T: Send + 'a>(&self, f: F) -> Receiver<T> {
        let (send, recv) = channel();
        self.new_task(move || f(send));
        recv
    }

    ///Stream data from a task, but limit the number of in-flight items.
    pub fn sync_stream<F: FnOnce(SyncSender<T>) + Send + 'a, T: Send + 'a>(&self, bound: usize, f: F) -> Receiver<T> {
        let (send, recv) = sync_channel(bound);
        self.new_task(move || f(send));
        recv
    }
}

trait FnBox {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<Self>) {
        (*self)()
    }
}

enum WorkerMessage<'env> {
    Task(Box<FnBox + Send + 'env>),
    End,
}
