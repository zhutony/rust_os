// "Tifflin" Kernel
// - By John Hodge (thePowersGang)
//
// Core/async/mod.rs
///! Asynchronous IO and waiting support
use _common::*;
use core::atomic::{AtomicBool,ATOMIC_BOOL_INIT};

pub use self::mutex::Mutex;

pub mod mutex;

pub mod events
{
	pub type EventMask = u32;
}

/// A general-purpose wait event (when flag is set, waiters will be informed)
pub struct EventSource
{
	flag: AtomicBool,
	waiter: ::sync::mutex::Mutex<Option<::threads::SleepObjectRef>>
}

pub struct EventWait<'a>
{
	source: &'a EventSource,
	callback: Option<Box<for<'r> ::lib::thunk::Invoke<(&'r mut EventWait<'a>),()> + Send + 'a>>,
}

/// A handle returned by a read operation (re-borrows the target buffer)
pub struct ReadHandle<'buf,'src>
{
	buffer: &'buf [u8],
	waiter: EventWait<'src>,
}

/// A handle returned by a read operation (re-borrows the target buffer)
pub struct WriteHandle<'buf,'src>
{
	buffer: &'buf [u8],
	waiter: EventWait<'src>,
}

pub enum WaitError
{
	Timeout,
}

impl EventSource
{
	pub fn new() -> EventSource
	{
		EventSource {
			flag: ATOMIC_BOOL_INIT,
			waiter: ::sync::mutex::Mutex::new(None),
		}
	}
	pub fn wait_on<'a, F: FnOnce(&mut EventWait) + Send + 'a>(&'a self, f: F) -> EventWait<'a>
	{
		EventWait {
			source: self,
			callback: Some(box f),
		}
	}
	pub fn trigger(&self)
	{
		self.flag.store(true, ::core::atomic::Ordering::Relaxed);
		self.waiter.lock().as_mut().map(|r| r.signal());
	}
}

impl<'a> EventWait<'a>
{
	pub fn is_valid(&self) -> bool
	{
		self.callback.is_some()
	}
	pub fn is_ready(&self) -> bool
	{
		self.is_valid() && self.source.flag.load(::core::atomic::Ordering::Relaxed)
	}
	
	pub fn bind_signal(&mut self, sleeper: &mut ::threads::SleepObject)
	{
		*self.source.waiter.lock() = Some(sleeper.get_ref());
	}
	
	pub fn run_completion(&mut self)
	{
		let callback = self.callback.take().expect("EventWait::run_completion with callback None");
		callback.invoke(self);
	}
	
	/// Call the provided function after the original callback
	pub fn chain<F: FnOnce(&mut EventWait) + Send + 'a>(mut self, f: F) -> EventWait<'a>
	{
		let cb = self.callback.take().unwrap();
		let newcb = box move |e: &mut EventWait<'a>| { cb.invoke(e); f(e); };
		EventWait {
			callback: Some(newcb),
			source: self.source,
		}
	}
}

impl<'o_b,'o_e> ReadHandle<'o_b, 'o_e>
{
	pub fn new<'b,'e>(dst: &'b [u8], w: EventWait<'e>) -> ReadHandle<'b,'e>
	{
		ReadHandle {
			buffer: dst,
			waiter: w,
		}
	}
}

impl<'o_b,'o_e> WriteHandle<'o_b, 'o_e>
{
	pub fn new<'b,'e>(dst: &'b [u8], w: EventWait<'e>) -> WriteHandle<'b,'e>
	{
		WriteHandle {
			buffer: dst,
			waiter: w,
		}
	}
}

// Note - List itself isn't modified, but needs to be &mut to get &mut to inners
pub fn wait_on_list(waiters: &mut [&mut EventWait])
{
	if waiters.len() == 0
	{
		panic!("wait_on_list - Nothing to wait on");
	}
	//else if waiters.len() == 1
	//{
	//	// Only one item to wait on, explicitly wait
	//	waiters[0].wait()
	//}
	else
	{
		// Multiple waiters
		// - Create an object for them to signal
		let mut obj = ::threads::SleepObject::new("wait_on_list");
		for ent in waiters.iter_mut()
		{
			ent.bind_signal( &mut obj );
		}
		
		// - Wait the current thread on that object
		obj.wait();
		
		// - When woken, run completion handlers on all completed waiters
		for ent in waiters.iter_mut()
		{
			if ent.is_ready()
			{
				ent.run_completion();
			}
		}
	}
}


