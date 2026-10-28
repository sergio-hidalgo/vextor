//! The "open documents" Apple event (`kAEOpenDocuments`).
//!
//! winit installs its own application delegate (which it needs to start),
//! so Vextor cannot take the delegate's `application:openURLs:`. Instead a
//! small handler object installs an Apple event handler for "open
//! documents" when the app posts "will finish launching": after
//! `NSApplication` has installed its default handlers (which would replace
//! an earlier one) and before a cold start's event is dispatched.

// The one module in the workspace that allows `unsafe` (spec 14b): every
// block says why it is sound.
#![allow(unsafe_code)]

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter,
    NSObjectProtocol, NSString,
};

/// Apple event codes (`AEDataModel.h`, `AppleEvents.h`): four ASCII
/// characters read as a big-endian `u32`.
const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

/// Posted by `-[NSApplication finishLaunching]`; the value of `AppKit`'s
/// `NSApplicationWillFinishLaunchingNotification` is its own name.
const WILL_FINISH_LAUNCHING: &str = "NSApplicationWillFinishLaunchingNotification";

/// Where the opened files go.
struct Ivars {
    sender: Sender<PathBuf>,
    wake: Box<dyn Fn()>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Handler` does
    // not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "VextorOpenDocumentsHandler"]
    #[ivars = Ivars]
    struct Handler;

    impl Handler {
        // SAFETY: the selector takes one object (the notification) and
        // returns nothing, as the signature says.
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            self.register();
        }

        // SAFETY: Apple event handlers are called with two
        // `NSAppleEventDescriptor`s (the event and the reply) and return
        // nothing, as the signature says.
        #[unsafe(method(openDocuments:withReplyEvent:))]
        fn open_documents(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            let ivars = self.ivars();
            let mut any = false;
            for path in file_paths(event) {
                any |= ivars.sender.send(path).is_ok();
            }
            if any {
                (ivars.wake)();
            }
        }
    }

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for Handler {}
);

impl Handler {
    fn new(mtm: MainThreadMarker, ivars: Ivars) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ivars);
        // SAFETY: `init` is NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }

    /// Makes this object the handler for "open documents".
    fn register(&self) {
        let manager = NSAppleEventManager::sharedAppleEventManager();
        let handler: &AnyObject = self.as_ref();
        // SAFETY: `setEventHandler:andSelector:forEventClass:andEventID:`
        // takes an object, a selector and two `FourCharCode`s (`u32`), and
        // returns nothing. `self` implements the selector with the handler
        // signature, and is never freed (`install` leaks it), so the
        // manager's unretained reference stays valid.
        unsafe {
            let () = msg_send![
                &manager,
                setEventHandler: handler,
                andSelector: sel!(openDocuments:withReplyEvent:),
                forEventClass: CORE_EVENT_CLASS,
                andEventID: OPEN_DOCUMENTS,
            ];
        }
    }
}

/// The local files named by an "open documents" event, in order. Anything
/// that is not a `file://` URL is skipped.
fn file_paths(event: &NSAppleEventDescriptor) -> Vec<PathBuf> {
    // SAFETY: `paramDescriptorForKeyword:` takes an `AEKeyword` (`u32`) and
    // returns a descriptor or nil.
    let list: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
    let Some(list) = list else {
        return Vec::new();
    };
    let count = list.numberOfItems();
    // A single file can come as the descriptor itself rather than a list.
    let items: Vec<_> = if count > 0 {
        (1..=count)
            .filter_map(|i| list.descriptorAtIndex(i))
            .collect()
    } else {
        vec![list]
    };
    items
        .iter()
        .filter_map(|item| item.fileURLValue())
        .filter(|url| url.isFileURL())
        .filter_map(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}

/// Sends every file macOS asks Vextor to open to `sender`, then calls
/// `wake` (from the main thread). Call once, from the main thread, before
/// the event loop runs; elsewhere it does nothing.
pub fn install(sender: Sender<PathBuf>, wake: impl Fn() + 'static) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let handler = Handler::new(
        mtm,
        Ivars {
            sender,
            wake: Box::new(wake),
        },
    );
    let observer: &AnyObject = (*handler).as_ref();
    let name = NSString::from_str(WILL_FINISH_LAUNCHING);
    // SAFETY: `handler` implements `willFinishLaunching:`, which takes the
    // notification; it is leaked below, so the centre's unretained
    // reference stays valid for the life of the process.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            observer,
            sel!(willFinishLaunching:),
            Some(&name),
            None,
        );
    }
    // Lives as long as the app: the notification centre and the Apple event
    // manager keep only unretained references to it.
    std::mem::forget(handler);
}
