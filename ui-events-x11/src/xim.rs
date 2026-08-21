// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! XIM input-method client producing [`TextInputEvent`]s.
//!
//! [`XimClient`] speaks the X Input Method protocol over the consumer's
//! `x11rb` connection, borrowed per call, like the crate's reducers.
//! Keys are forwarded to the IM server.
//! The server answers with committed text, preedit updates, or the key itself
//! when it declines to consume it.
//!
//! Drive it from the event loop:
//!
//! - [`connect`](XimClient::connect) finds the server named by `XMODIFIERS`
//!   (or the first advertised server) and starts the handshake.
//!   `None` means no input method is available.
//! - Pass every decoded event to [`filter_event`](XimClient::filter_event)
//!   first.
//!   `true` means the event belonged to the input method.
//! - Offer XInput 2 key events to [`filter_key`](XimClient::filter_key).
//!   `true` means the key was forwarded and is withheld until the server
//!   responds.
//! - Drain [`take_event`](XimClient::take_event) after either call.
//!   [`XimEvent::Text`] is committed text and composition updates.
//!   [`XimEvent::Key`] is a declined key for normal processing.
//! - Report focus with [`set_focus`](XimClient::set_focus) and the caret with
//!   [`set_caret`](XimClient::set_caret).
//!   Spot is the insertion baseline.
//!   Area is the caret rectangle.
//!   The two preedit attributes are independent.
//!
//! Prefers on-the-spot style (application-drawn preedit) when the server
//! offers it, otherwise no-preedit (server-drawn preedit at the caret).
//! Text is UTF-8 when the server accepts `UTF8_STRING`, else `COMPOUND_TEXT`.
//!
//! If the server's selection-owner window is destroyed, filtering stops and
//! [`is_alive`](XimClient::is_alive) is `false`.
//! Drop the client and reconnect when a server reappears.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use ui_events::text::{CompositionState, TextInputEvent, TextInsertEvent, TextRange};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput;
use x11rb::protocol::xproto::{
    self, Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _,
    CreateWindowAux, EventMask, PropMode, Window, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::x11_utils::TryParse;

mod ctext;
mod proto;

/// `XIMPreeditCallbacks | XIMStatusNothing`: the application draws the
/// preedit from composition updates.
const STYLE_ON_THE_SPOT: u32 = 0x0002 | 0x0400;
/// `XIMPreeditNothing | XIMStatusNothing`: the server draws its own preedit.
const STYLE_NO_PREEDIT: u32 = 0x0008 | 0x0400;

/// Core protocol event mask bit for `KeyPress`.
const KEY_PRESS_MASK: u32 = 0x0000_0001;
/// Core protocol event mask bit for `KeyRelease`.
const KEY_RELEASE_MASK: u32 = 0x0000_0002;

/// An event produced by the input method.
#[derive(Debug)]
pub enum XimEvent {
    /// Committed text or a composition update.
    Text(TextInputEvent),
    /// A forwarded key the server declined to consume.
    ///
    /// An XInput 2 key event (identity, time, modifiers).
    /// Handle it with [`KeyboardEventReducer::reduce`].
    ///
    /// [`KeyboardEventReducer::reduce`]: crate::keyboard::KeyboardEventReducer::reduce
    Key(Event),
}

/// Progress of the connection with the IM server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// `_XIM_XCONNECT` sent; waiting for the server's transport reply.
    AwaitTransport,
    /// `XIM_CONNECT` sent.
    AwaitConnectReply,
    /// `XIM_OPEN` sent.
    AwaitOpenReply,
    /// `XIM_ENCODING_NEGOTIATION` sent.
    AwaitEncodingReply,
    /// `XIM_GET_IM_VALUES` querying the supported input styles sent.
    AwaitStyles,
    /// `XIM_CREATE_IC` sent.
    AwaitIc,
    /// The input context exists; keys are filtered and text flows.
    Ready,
    /// The connection failed or the server went away; the client is inert.
    Dead,
}

/// Spot baseline and area rectangle last sent to the server.
type SentCaret = ((i16, i16), (i16, i16, u16, u16));

/// An XIM client bound to one top-level window.
///
/// See the [module documentation](self) for how to drive it.
#[derive(Debug)]
pub struct XimClient {
    /// `_XIM_XCONNECT` atom: transport setup client messages.
    atom_xconnect: Atom,
    /// `_XIM_PROTOCOL` atom: XIM packet client messages (inline or property).
    atom_protocol: Atom,
    /// `_XIM_MOREDATA` atom: non-final chunks of a multi-message transfer.
    atom_moredata: Atom,
    /// Property atom for client-to-server packets longer than 20 bytes.
    transfer_property: Atom,
    /// Selection owner of the chosen XIM server, watched for destruction.
    server_owner: Window,
    /// Server communication window from the `_XIM_XCONNECT` reply.
    /// `x11rb::NONE` until the transport handshake completes.
    ims_window: Window,
    /// This client's 1x1 communication window; server packets arrive here.
    comm_window: Window,
    /// Application top-level window bound as `clientWindow` / `focusWindow`.
    client_window: Window,
    /// Root window from the connection setup (parent of [`Self::comm_window`]).
    root: Window,
    /// Locale name for `XIM_OPEN`; cleared once that packet is sent.
    locale: String,
    /// Handshake / runtime phase of the connection.
    phase: Phase,
    /// Input-method id from `XIM_OPEN_REPLY` (`0` until then).
    im_id: u16,
    /// Input-context id from `XIM_CREATE_IC_REPLY` (`0` until then).
    ic_id: u16,
    /// IM attribute id for `queryInputStyle`, if the server advertises it.
    attr_query_input_style: Option<u16>,
    /// IC attribute id for `inputStyle`.
    attr_input_style: Option<u16>,
    /// IC attribute id for `clientWindow`.
    attr_client_window: Option<u16>,
    /// IC attribute id for `focusWindow`.
    attr_focus_window: Option<u16>,
    /// IC attribute id for nested `preeditAttributes`.
    attr_preedit: Option<u16>,
    /// Nested preedit attribute id for `spotLocation` (caret baseline point).
    attr_spot: Option<u16>,
    /// Nested preedit attribute id for `area` (caret rectangle).
    attr_area: Option<u16>,
    /// Core event mask from `XIM_SET_EVENT_MASK`: which keys to forward.
    forward_mask: u32,
    /// Core event mask from `XIM_SET_EVENT_MASK`: keys that need
    /// `XIM_SYNC_REPLY` after `XIM_FORWARD_EVENT`.
    sync_mask: u32,
    /// Whether wire text is `UTF8_STRING`.
    /// `false` means `COMPOUND_TEXT`.
    utf8_wire: bool,
    /// In-progress preedit string, in characters.
    /// XIM draw ranges and carets are character indices, not bytes.
    preedit: Vec<char>,
    /// Caret index within [`Self::preedit`], in characters.
    caret: usize,
    /// Whether a composition update has been emitted and not yet ended.
    composing: bool,
    /// Host-reported keyboard focus; applied as `XIM_SET_IC_FOCUS` once Ready.
    focused: bool,
    /// Latest insertion-caret baseline from [`Self::set_caret`]
    /// (`x`, `y` in window pixels), if any.
    caret_spot: Option<(i16, i16)>,
    /// Latest caret rectangle from [`Self::set_caret`]
    /// (`x`, `y`, `width`, `height` in window pixels), if any.
    caret_area: Option<(i16, i16, u16, u16)>,
    /// Last caret geometry successfully sent to the server (dedupes updates).
    sent_caret: Option<SentCaret>,
    /// Reassembly buffer for packets spanning transport chunks.
    incoming: Vec<u8>,
    /// Output queue drained by [`Self::take_event`].
    events: VecDeque<XimEvent>,
}

impl XimClient {
    /// Discover an IM server and start connecting to it.
    ///
    /// `server_name` is the `@im=` value from `XMODIFIERS`, matching a name in
    /// the root window's `XIM_SERVERS` property.
    /// `None` uses the first advertised server.
    /// `locale` is announced to the server (for example `en_US.UTF-8`).
    /// Returns `None` when no matching server is available.
    ///
    /// The handshake is asynchronous: keep feeding [`filter_event`](Self::filter_event).
    pub fn connect(
        conn: &impl Connection,
        client_window: Window,
        server_name: Option<&str>,
        locale: &str,
    ) -> Option<Self> {
        let root = conn.setup().roots.first()?.root;
        let atom_servers = intern(conn, b"XIM_SERVERS")?;
        let atom_xconnect = intern(conn, b"_XIM_XCONNECT")?;
        let atom_protocol = intern(conn, b"_XIM_PROTOCOL")?;
        let atom_moredata = intern(conn, b"_XIM_MOREDATA")?;

        let servers = conn
            .get_property(false, root, atom_servers, AtomEnum::ATOM, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        let atoms: Box<[Atom]> = servers.value32()?.collect();
        let server_atom = choose_server(conn, &atoms, server_name)?;
        let server_owner = conn
            .get_selection_owner(server_atom)
            .ok()?
            .reply()
            .ok()?
            .owner;
        if server_owner == x11rb::NONE {
            return None;
        }

        let comm_window = conn.generate_id().ok()?;
        conn.create_window(
            0,
            comm_window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .ok()?;
        let _ = conn.change_window_attributes(
            server_owner,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
        );
        let transfer_property = intern(conn, format!("_client{comm_window}").as_bytes())?;

        let message =
            ClientMessageEvent::new(32, server_owner, atom_xconnect, [comm_window, 0, 0, 0, 0]);
        conn.send_event(false, server_owner, EventMask::NO_EVENT, message)
            .ok()?;
        conn.flush().ok()?;

        Some(Self {
            atom_xconnect,
            atom_protocol,
            atom_moredata,
            transfer_property,
            server_owner,
            ims_window: x11rb::NONE,
            comm_window,
            client_window,
            root,
            locale: locale.into(),
            phase: Phase::AwaitTransport,
            im_id: 0,
            ic_id: 0,
            attr_query_input_style: None,
            attr_input_style: None,
            attr_client_window: None,
            attr_focus_window: None,
            attr_preedit: None,
            attr_spot: None,
            attr_area: None,
            forward_mask: 0,
            sync_mask: 0,
            utf8_wire: false,
            preedit: Vec::new(),
            caret: 0,
            composing: false,
            focused: false,
            caret_spot: None,
            caret_area: None,
            sent_caret: None,
            incoming: Vec::new(),
            events: VecDeque::new(),
        })
    }

    /// Whether the connection is still usable.
    ///
    /// `false` once the handshake has failed or the server has gone away.
    /// Drop the client then, and reconnect when the root window's
    /// `XIM_SERVERS` property changes.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.phase != Phase::Dead
    }

    /// Handle an event belonging to the input-method transport.
    ///
    /// Feed every decoded event here first.
    /// Returns `true` if the event was for the input method (transport on the
    /// communication window, or structure notify on the server's windows).
    /// May queue events for [`take_event`](Self::take_event).
    pub fn filter_event(&mut self, conn: &impl Connection, event: &Event) -> bool {
        if self.phase == Phase::Dead {
            return false;
        }
        match event {
            Event::ClientMessage(message) if message.window == self.comm_window => {
                self.client_message(conn, message);
                true
            }
            Event::DestroyNotify(destroy) if self.is_server_window(destroy.window) => {
                self.phase = Phase::Dead;
                true
            }
            Event::ConfigureNotify(notify) if self.is_server_window(notify.window) => true,
            Event::MapNotify(notify) if self.is_server_window(notify.window) => true,
            Event::UnmapNotify(notify) if self.is_server_window(notify.window) => true,
            Event::ReparentNotify(notify) if self.is_server_window(notify.window) => true,
            Event::GravityNotify(notify) if self.is_server_window(notify.window) => true,
            Event::CirculateNotify(notify) if self.is_server_window(notify.window) => true,
            _ => false,
        }
    }

    /// Offer an XInput 2 key event to the input method.
    ///
    /// Return `true` when the key was forwarded to the server and must be
    /// withheld from normal processing.
    /// The server later answers with text or returns the key through
    /// [`XimEvent::Key`].
    /// Return `false` for non-key events and whenever the input method is not
    /// filtering.
    pub fn filter_key(&mut self, conn: &impl Connection, event: &Event) -> bool {
        let (press, key) = match event {
            Event::XinputKeyPress(key) => (true, key),
            Event::XinputKeyRelease(key) => (false, key),
            _ => return false,
        };
        if self.phase != Phase::Ready {
            return false;
        }
        let mask_bit = if press {
            KEY_PRESS_MASK
        } else {
            KEY_RELEASE_MASK
        };
        if self.forward_mask & mask_bit == 0 {
            return false;
        }
        let Ok(keycode) = u8::try_from(key.detail) else {
            return false;
        };
        let core = self.core_key_event(press, keycode, key);
        let flags = if self.sync_mask & mask_bit != 0 {
            proto::FLAG_SYNCHRONOUS
        } else {
            0
        };
        self.send(
            conn,
            proto::forward_event_packet(self.im_id, self.ic_id, flags, &core),
        );
        self.phase == Phase::Ready
    }

    /// Take the next queued input-method event, if any.
    ///
    /// Drain this after [`filter_event`](Self::filter_event) or
    /// [`filter_key`](Self::filter_key) returned `true`.
    pub fn take_event(&mut self) -> Option<XimEvent> {
        self.events.pop_front()
    }

    /// Report whether the window has keyboard focus.
    ///
    /// Call on focus changes.
    /// The focused state is also applied when the input context becomes ready.
    pub fn set_focus(&mut self, conn: &impl Connection, focused: bool) {
        self.focused = focused;
        if self.phase != Phase::Ready {
            return;
        }
        let major = if focused {
            proto::XIM_SET_IC_FOCUS
        } else {
            proto::XIM_UNSET_IC_FOCUS
        };
        self.send(conn, proto::ids_packet(major, self.im_id, self.ic_id));
    }

    /// Report the insertion caret baseline and caret rectangle, in pixels
    /// relative to the client window.
    ///
    /// `spot_*` is `spotLocation` (insertion baseline, including inside preedit).
    /// `area_*` is `area` (caret rectangle for candidate/over-the-spot placement).
    /// The two are independent; the spot is not assumed to be a corner of the area.
    /// Identical geometry is not resent.
    pub fn set_caret(
        &mut self,
        conn: &impl Connection,
        spot_x: i16,
        spot_y: i16,
        area_x: i16,
        area_y: i16,
        area_width: u16,
        area_height: u16,
    ) {
        let spot = (spot_x, spot_y);
        let area = (area_x, area_y, area_width, area_height);
        if self.sent_caret == Some((spot, area)) {
            return;
        }
        self.caret_spot = Some(spot);
        self.caret_area = Some(area);
        if self.phase == Phase::Ready {
            self.send_caret(conn);
        }
    }

    /// End the session, destroying the input context and disconnecting.
    ///
    /// Best-effort: the teardown requests are sent without waiting for their
    /// replies.
    /// The client is inert afterwards.
    pub fn shutdown(&mut self, conn: &impl Connection) {
        if self.phase == Phase::Ready {
            self.send(
                conn,
                proto::ids_packet(proto::XIM_DESTROY_IC, self.im_id, self.ic_id),
            );
        }
        if self.opened() {
            self.send(conn, proto::close_packet(self.im_id));
        }
        if self.phase != Phase::AwaitTransport {
            self.send(conn, proto::disconnect_packet());
        }
        let _ = conn.destroy_window(self.comm_window);
        let _ = conn.flush();
        self.phase = Phase::Dead;
    }

    /// Whether `XIM_OPEN` has completed, so an input-method id exists.
    fn opened(&self) -> bool {
        matches!(
            self.phase,
            Phase::AwaitEncodingReply | Phase::AwaitStyles | Phase::AwaitIc | Phase::Ready
        )
    }

    /// Whether `window` is one of the server's windows this client watches.
    fn is_server_window(&self, window: Window) -> bool {
        window != x11rb::NONE && (window == self.server_owner || window == self.ims_window)
    }

    /// Handle a transport client message addressed to the comm window.
    fn client_message(&mut self, conn: &impl Connection, message: &ClientMessageEvent) {
        if message.type_ == self.atom_xconnect && message.format == 32 {
            if self.phase != Phase::AwaitTransport {
                return;
            }
            self.ims_window = message.data.as_data32()[0];
            if self.ims_window != self.server_owner {
                let _ = conn.change_window_attributes(
                    self.ims_window,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
                );
            }
            self.phase = Phase::AwaitConnectReply;
            self.send(conn, proto::connect_packet());
        } else if message.type_ == self.atom_protocol && message.format == 32 {
            // Large server data spilled into a property on the comm window.
            let data = message.data.as_data32();
            let length = data[0] as usize;
            let property = data[1];
            let Ok(cookie) = conn.get_property(
                true,
                self.comm_window,
                property,
                AtomEnum::ANY,
                0,
                u32::try_from(length.div_ceil(4)).unwrap_or(u32::MAX),
            ) else {
                self.phase = Phase::Dead;
                return;
            };
            let Ok(reply) = cookie.reply() else {
                self.phase = Phase::Dead;
                return;
            };
            let value = &reply.value[..length.min(reply.value.len())];
            self.ingest(conn, value, true);
        } else if message.type_ == self.atom_protocol && message.format == 8 {
            self.ingest(conn, &message.data.as_data8(), true);
        } else if message.type_ == self.atom_moredata && message.format == 8 {
            self.ingest(conn, &message.data.as_data8(), false);
        }
    }

    /// Append transport bytes and handle every complete packet.
    ///
    /// `last` marks the end of a transfer, whose trailing zero padding is then
    /// discarded.
    /// A packet may span chunks within one transfer.
    fn ingest(&mut self, conn: &impl Connection, bytes: &[u8], last: bool) {
        self.incoming.extend_from_slice(bytes);
        let buf = core::mem::take(&mut self.incoming);
        let mut offset = 0;
        while let Some((major, body, consumed)) = proto::split_packet(&buf[offset..]) {
            self.handle_packet(conn, major, body);
            offset += consumed;
        }
        if !last {
            self.incoming = buf[offset..].to_vec();
        }
    }

    /// Advance the handshake or handle a runtime message.
    fn handle_packet(&mut self, conn: &impl Connection, major: u8, body: &[u8]) {
        match major {
            proto::XIM_ERROR => {
                // A handshake step failed; at runtime errors are non-fatal
                // (for example a rejected attribute update).
                if self.phase != Phase::Ready {
                    self.phase = Phase::Dead;
                }
            }
            proto::XIM_CONNECT_REPLY if self.phase == Phase::AwaitConnectReply => {
                self.phase = Phase::AwaitOpenReply;
                let locale = core::mem::take(&mut self.locale);
                self.send(conn, proto::open_packet(&locale));
            }
            proto::XIM_OPEN_REPLY if self.phase == Phase::AwaitOpenReply => {
                self.open_reply(conn, body);
            }
            proto::XIM_ENCODING_NEGOTIATION_REPLY if self.phase == Phase::AwaitEncodingReply => {
                if let Some((category, index)) = proto::parse_encoding_reply(body) {
                    self.utf8_wire = category == 0 && index == proto::ENCODING_INDEX_UTF8;
                }
                self.phase = Phase::AwaitStyles;
                let attr = self.attr_query_input_style.unwrap_or(0);
                self.send(conn, proto::get_im_values_packet(self.im_id, attr));
            }
            proto::XIM_GET_IM_VALUES_REPLY if self.phase == Phase::AwaitStyles => {
                self.styles_reply(conn, body);
            }
            proto::XIM_CREATE_IC_REPLY if self.phase == Phase::AwaitIc => {
                let Some((_, ic_id)) = proto::parse_create_ic_reply(body) else {
                    self.phase = Phase::Dead;
                    return;
                };
                self.ic_id = ic_id;
                self.phase = Phase::Ready;
                if self.focused {
                    self.send(
                        conn,
                        proto::ids_packet(proto::XIM_SET_IC_FOCUS, self.im_id, self.ic_id),
                    );
                }
                self.send_caret(conn);
            }
            proto::XIM_SET_EVENT_MASK => {
                if let Some((_, _, forward, sync)) = proto::parse_set_event_mask(body) {
                    self.forward_mask = forward;
                    self.sync_mask = sync;
                }
            }
            proto::XIM_FORWARD_EVENT if self.phase == Phase::Ready => {
                if let Some((_, _, flags, wire)) = proto::parse_forward_event(body) {
                    if let Some(key) = reinjected_key(&wire) {
                        self.events.push_back(XimEvent::Key(key));
                    }
                    self.sync_reply_if_requested(conn, flags);
                }
            }
            proto::XIM_COMMIT if self.phase == Phase::Ready => {
                if let Some((_, _, flags, bytes)) = proto::parse_commit(body) {
                    let text = self.decode_text(&bytes);
                    if !text.is_empty() {
                        self.events.push_back(XimEvent::Text(TextInputEvent::Insert(
                            TextInsertEvent::new(text),
                        )));
                    }
                    self.sync_reply_if_requested(conn, flags);
                }
            }
            proto::XIM_SYNC => {
                self.send(
                    conn,
                    proto::ids_packet(proto::XIM_SYNC_REPLY, self.im_id, self.ic_id),
                );
            }
            proto::XIM_PREEDIT_START if self.phase == Phase::Ready => {
                self.preedit.clear();
                self.caret = 0;
                // A negative return value places no limit on the preedit length.
                self.send(
                    conn,
                    proto::preedit_start_reply_packet(self.im_id, self.ic_id, -1),
                );
            }
            proto::XIM_PREEDIT_DRAW if self.phase == Phase::Ready => {
                if let Some((_, _, draw)) = proto::parse_preedit_draw(body) {
                    self.apply_draw(&draw);
                }
            }
            proto::XIM_PREEDIT_CARET if self.phase == Phase::Ready => {
                if let Some((_, _, position, direction)) = proto::parse_preedit_caret(body) {
                    if direction == proto::CARET_ABSOLUTE_POSITION {
                        self.caret = usize::try_from(position).unwrap_or(0);
                        self.emit_preedit();
                    }
                    let position = u32::try_from(self.caret).unwrap_or(0);
                    self.send(
                        conn,
                        proto::preedit_caret_reply_packet(self.im_id, self.ic_id, position),
                    );
                }
            }
            proto::XIM_PREEDIT_DONE if self.phase == Phase::Ready => {
                self.preedit.clear();
                self.caret = 0;
                if self.composing {
                    self.composing = false;
                    self.events
                        .push_back(XimEvent::Text(TextInputEvent::CompositionEnd));
                }
            }
            proto::XIM_REGISTER_TRIGGERKEYS
            | proto::XIM_SYNC_REPLY
            | proto::XIM_SET_IC_VALUES_REPLY => {}
            _ => {}
        }
    }

    /// `XIM_OPEN_REPLY`: resolve attribute ids and negotiate the wire encoding.
    fn open_reply(&mut self, conn: &impl Connection, body: &[u8]) {
        let Some(reply) = proto::parse_open_reply(body) else {
            self.phase = Phase::Dead;
            return;
        };
        self.im_id = reply.im_id;
        self.attr_query_input_style = attr_id(&reply.im_attrs, b"queryInputStyle");
        self.attr_input_style = attr_id(&reply.ic_attrs, b"inputStyle");
        self.attr_client_window = attr_id(&reply.ic_attrs, b"clientWindow");
        self.attr_focus_window = attr_id(&reply.ic_attrs, b"focusWindow");
        self.attr_preedit = attr_id(&reply.ic_attrs, b"preeditAttributes");
        self.attr_spot = attr_id(&reply.ic_attrs, b"spotLocation");
        self.attr_area = attr_id(&reply.ic_attrs, b"area");
        if self.attr_query_input_style.is_none()
            || self.attr_input_style.is_none()
            || self.attr_client_window.is_none()
        {
            self.phase = Phase::Dead;
            return;
        }
        self.phase = Phase::AwaitEncodingReply;
        self.send(conn, proto::encoding_negotiation_packet(self.im_id));
    }

    /// Create the IC from the queried input styles.
    fn styles_reply(&mut self, conn: &impl Connection, body: &[u8]) {
        let styles = self
            .attr_query_input_style
            .and_then(|attr| proto::parse_styles_reply(body, attr))
            .unwrap_or_else(|| Box::from([]));
        let Some(style) = choose_style(&styles) else {
            // Unsupported style.
            self.send(conn, proto::close_packet(self.im_id));
            self.send(conn, proto::disconnect_packet());
            self.phase = Phase::Dead;
            return;
        };
        let attrs: Box<[_]> = self
            .attr_input_style
            .map(|id| (id, proto::u32_value(style)))
            .into_iter()
            .chain(
                self.attr_client_window
                    .map(|id| (id, proto::u32_value(self.client_window))),
            )
            .chain(
                self.attr_focus_window
                    .map(|id| (id, proto::u32_value(self.client_window))),
            )
            .collect();
        self.phase = Phase::AwaitIc;
        self.send(conn, proto::create_ic_packet(self.im_id, &attrs));
    }

    /// Send `XIM_SYNC_REPLY` when the server flagged a message synchronous.
    fn sync_reply_if_requested(&mut self, conn: &impl Connection, flags: u16) {
        if flags & proto::FLAG_SYNCHRONOUS != 0 {
            self.send(
                conn,
                proto::ids_packet(proto::XIM_SYNC_REPLY, self.im_id, self.ic_id),
            );
        }
    }

    /// Splice the draw's change range and move the caret.
    fn apply_draw(&mut self, draw: &proto::PreeditDraw) {
        let text = if draw.status & proto::DRAW_NO_STRING != 0 {
            String::new()
        } else {
            self.decode_text(&draw.text)
        };
        let start = usize::try_from(draw.chg_first)
            .unwrap_or(0)
            .min(self.preedit.len());
        let length = usize::try_from(draw.chg_length).unwrap_or(0);
        let end = start.saturating_add(length).min(self.preedit.len());
        let tail = self.preedit.split_off(end);
        self.preedit.truncate(start);
        self.preedit.extend(text.chars());
        self.preedit.extend(tail);
        if let Ok(caret) = usize::try_from(draw.caret) {
            self.caret = caret;
        }
        self.emit_preedit();
    }

    /// Queue a composition update, or `CompositionEnd` if the preedit is empty.
    fn emit_preedit(&mut self) {
        if self.preedit.is_empty() {
            if self.composing {
                self.composing = false;
                self.events
                    .push_back(XimEvent::Text(TextInputEvent::CompositionEnd));
            }
            return;
        }
        self.composing = true;
        let caret_chars = self.caret.min(self.preedit.len());
        let caret_bytes: usize = self.preedit[..caret_chars]
            .iter()
            .map(|c| c.len_utf8())
            .sum();
        let caret = u32::try_from(caret_bytes).unwrap_or(0);
        let text: String = self.preedit.iter().collect();
        let state = CompositionState::new(text).with_selection(TextRange::new(caret, caret));
        self.events
            .push_back(XimEvent::Text(TextInputEvent::CompositionUpdate(state)));
    }

    /// Decode wire text in the negotiated encoding.
    fn decode_text(&self, bytes: &[u8]) -> String {
        if self.utf8_wire {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            ctext::decode(bytes)
        }
    }

    /// Send stored spot and area as preedit attributes.
    fn send_caret(&mut self, conn: &impl Connection) {
        let Some((spot_x, spot_y)) = self.caret_spot else {
            return;
        };
        let Some((area_x, area_y, width, height)) = self.caret_area else {
            return;
        };
        let Some(preedit) = self.attr_preedit else {
            return;
        };
        let nested: Box<[_]> = self
            .attr_spot
            .map(|id| (id, proto::point_value(spot_x, spot_y)))
            .into_iter()
            .chain(
                self.attr_area
                    .map(|id| (id, proto::rectangle_value(area_x, area_y, width, height))),
            )
            .collect();
        if nested.is_empty() {
            return;
        }
        let value = proto::nested_value(&nested);
        self.send(
            conn,
            proto::set_ic_values_packet(self.im_id, self.ic_id, &[(preedit, value)]),
        );
        self.sent_caret = Some(((spot_x, spot_y), (area_x, area_y, width, height)));
    }

    /// Serialized core key event for `XIM_FORWARD_EVENT`.
    fn core_key_event(&self, press: bool, keycode: u8, key: &xinput::KeyPressEvent) -> [u8; 32] {
        // Effective modifiers; layout group in bits 13..14.
        let state = (key.mods.effective & 0x1FFF) | ((u32::from(key.group.effective) & 0x3) << 13);
        let event = xproto::KeyPressEvent {
            response_type: if press {
                xproto::KEY_PRESS_EVENT
            } else {
                xproto::KEY_RELEASE_EVENT
            },
            detail: keycode,
            sequence: 0,
            time: key.time,
            root: self.root,
            event: self.client_window,
            child: x11rb::NONE,
            root_x: fp1616_whole(key.root_x),
            root_y: fp1616_whole(key.root_y),
            event_x: fp1616_whole(key.event_x),
            event_y: fp1616_whole(key.event_y),
            state: xproto::KeyButMask::from(u16::try_from(state).unwrap_or(0)),
            same_screen: true,
        };
        event.into()
    }

    /// Send one packet to the server, marking the client dead on failure.
    fn send(&mut self, conn: &impl Connection, packet: impl AsRef<[u8]>) {
        if self.phase == Phase::Dead || self.ims_window == x11rb::NONE {
            return;
        }
        if self.try_send(conn, packet.as_ref()).is_none() {
            self.phase = Phase::Dead;
        }
    }

    /// Write a packet: client message if at most 20 bytes, else a property.
    fn try_send(&self, conn: &impl Connection, packet: &[u8]) -> Option<()> {
        if packet.len() <= 20 {
            let mut data = [0_u8; 20];
            data[..packet.len()].copy_from_slice(packet);
            let message = ClientMessageEvent::new(8, self.ims_window, self.atom_protocol, data);
            conn.send_event(false, self.ims_window, EventMask::NO_EVENT, message)
                .ok()?;
        } else {
            conn.change_property8(
                PropMode::APPEND,
                self.ims_window,
                self.transfer_property,
                AtomEnum::STRING,
                packet,
            )
            .ok()?;
            let announce = [
                u32::try_from(packet.len()).unwrap_or(0),
                self.transfer_property,
                0,
                0,
                0,
            ];
            let message =
                ClientMessageEvent::new(32, self.ims_window, self.atom_protocol, announce);
            conn.send_event(false, self.ims_window, EventMask::NO_EVENT, message)
                .ok()?;
        }
        conn.flush().ok()
    }
}

/// Intern `name`, creating the atom if needed.
fn intern(conn: &impl Connection, name: &[u8]) -> Option<Atom> {
    Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom)
}

/// Pick the server's selection atom from the `XIM_SERVERS` list.
///
/// With a name, only `@server=<name>` matches.
/// Without one, the first advertised server is used.
fn choose_server(conn: &impl Connection, atoms: &[Atom], name: Option<&str>) -> Option<Atom> {
    let Some(name) = name else {
        return atoms.first().copied();
    };
    let target = format!("@server={name}");
    atoms.iter().copied().find(|&atom| {
        conn.get_atom_name(atom)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| reply.name == target.as_bytes())
    })
}

/// Look up an attribute id by name in an `XIM_OPEN_REPLY` dictionary.
fn attr_id(attrs: &[proto::AttrDefinition], name: &[u8]) -> Option<u16> {
    attrs
        .iter()
        .find(|attr| &*attr.name == name)
        .map(|attr| attr.id)
}

/// Pick the preferred input style the server offers, on-the-spot first.
fn choose_style(styles: &[u32]) -> Option<u32> {
    [STYLE_ON_THE_SPOT, STYLE_NO_PREEDIT]
        .into_iter()
        .find(|preferred| styles.contains(preferred))
}

/// Truncate an XInput 2 16.16 fixed-point coordinate to its whole part.
fn fp1616_whole(value: i32) -> i16 {
    i16::try_from(value >> 16).unwrap_or(0)
}

/// Rebuild an XInput 2 key event from a core key event the server returned.
///
/// Identity, timestamp, and modifiers come from the core event.
/// Group bits in core state become the locked layout group.
fn reinjected_key(wire: &[u8; 32]) -> Option<Event> {
    let (core, _) = xproto::KeyPressEvent::try_parse(wire.as_slice()).ok()?;
    let press = match core.response_type & 0x7F {
        xproto::KEY_PRESS_EVENT => true,
        xproto::KEY_RELEASE_EVENT => false,
        _ => return None,
    };
    let state = u32::from(u16::from(core.state));
    let key = xinput::KeyPressEvent {
        detail: u32::from(core.detail),
        time: core.time,
        mods: xinput::ModifierInfo {
            base: state & 0x1FFF,
            latched: 0,
            locked: 0,
            effective: state & 0x1FFF,
        },
        group: xinput::GroupInfo {
            base: 0,
            latched: 0,
            locked: u8::try_from((state >> 13) & 0x3).unwrap_or(0),
            effective: u8::try_from((state >> 13) & 0x3).unwrap_or(0),
        },
        ..Default::default()
    };
    Some(if press {
        Event::XinputKeyPress(key)
    } else {
        Event::XinputKeyRelease(key)
    })
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec::Vec;

    use ui_events::text::{TextInputEvent, TextRange};
    use x11rb::protocol::Event;

    use super::proto::PreeditDraw;
    use super::*;

    fn ready_client() -> XimClient {
        XimClient {
            atom_xconnect: 1,
            atom_protocol: 2,
            atom_moredata: 3,
            transfer_property: 4,
            server_owner: 100,
            ims_window: 101,
            comm_window: 102,
            client_window: 103,
            root: 104,
            locale: String::new(),
            phase: Phase::Ready,
            im_id: 1,
            ic_id: 1,
            attr_query_input_style: Some(1),
            attr_input_style: Some(2),
            attr_client_window: Some(3),
            attr_focus_window: Some(4),
            attr_preedit: Some(5),
            attr_spot: Some(6),
            attr_area: Some(7),
            forward_mask: KEY_PRESS_MASK,
            sync_mask: 0,
            utf8_wire: true,
            preedit: Vec::new(),
            caret: 0,
            composing: false,
            focused: false,
            caret_spot: None,
            caret_area: None,
            sent_caret: None,
            incoming: Vec::new(),
            events: VecDeque::new(),
        }
    }

    fn draw(text: &str, chg_first: i32, chg_length: i32, caret: i32) -> PreeditDraw {
        PreeditDraw {
            caret,
            chg_first,
            chg_length,
            status: 0,
            text: Box::from(text.as_bytes()),
        }
    }

    #[test]
    fn preedit_draws_splice_and_report_the_caret_in_bytes() {
        let mut client = ready_client();
        client.apply_draw(&draw("にほ", 0, 0, 2));
        let Some(XimEvent::Text(TextInputEvent::CompositionUpdate(state))) = client.take_event()
        else {
            panic!("expected a composition update");
        };
        assert_eq!(state.text, "にほ");
        assert_eq!(state.selection, Some(TextRange::new(6, 6)));

        client.apply_draw(&draw("本", 1, 1, 1));
        let Some(XimEvent::Text(TextInputEvent::CompositionUpdate(state))) = client.take_event()
        else {
            panic!("expected a composition update");
        };
        assert_eq!(state.text, "に本");
        assert_eq!(state.selection, Some(TextRange::new(3, 3)));
    }

    #[test]
    fn deleting_the_whole_preedit_ends_the_composition() {
        let mut client = ready_client();
        client.apply_draw(&draw("a", 0, 0, 1));
        let _ = client.take_event();
        client.apply_draw(&PreeditDraw {
            caret: 0,
            chg_first: 0,
            chg_length: 1,
            status: super::proto::DRAW_NO_STRING,
            text: Box::from([]),
        });
        assert!(matches!(
            client.take_event(),
            Some(XimEvent::Text(TextInputEvent::CompositionEnd))
        ));
        client.apply_draw(&draw("", 0, 0, 0));
        assert!(client.take_event().is_none());
    }

    #[test]
    fn out_of_range_draw_offsets_are_clamped() {
        let mut client = ready_client();
        client.apply_draw(&draw("ab", 5, 9, -3));
        let Some(XimEvent::Text(TextInputEvent::CompositionUpdate(state))) = client.take_event()
        else {
            panic!("expected a composition update");
        };
        assert_eq!(state.text, "ab");
    }

    #[test]
    fn reinjected_keys_round_trip_identity_and_modifiers() {
        let client = ready_client();
        let source = xinput::KeyPressEvent {
            detail: 38,
            time: 1234,
            mods: xinput::ModifierInfo {
                base: 0x1,
                latched: 0,
                locked: 0,
                effective: 0x1,
            },
            group: xinput::GroupInfo {
                base: 0,
                latched: 0,
                locked: 1,
                effective: 1,
            },
            ..Default::default()
        };
        let wire = client.core_key_event(true, 38, &source);
        let Some(Event::XinputKeyPress(key)) = reinjected_key(&wire) else {
            panic!("expected a reinjected key press");
        };
        assert_eq!(key.detail, 38);
        assert_eq!(key.time, 1234);
        assert_eq!(key.mods.effective, 0x1);
        assert_eq!(key.group.effective, 1);

        let wire = client.core_key_event(false, 38, &source);
        assert!(matches!(
            reinjected_key(&wire),
            Some(Event::XinputKeyRelease(_))
        ));
    }

    #[test]
    fn style_choice_prefers_on_the_spot() {
        assert_eq!(
            choose_style(&[STYLE_NO_PREEDIT, STYLE_ON_THE_SPOT]),
            Some(STYLE_ON_THE_SPOT)
        );
        assert_eq!(choose_style(&[STYLE_NO_PREEDIT]), Some(STYLE_NO_PREEDIT));
        assert_eq!(choose_style(&[0x0004 | 0x0400]), None);
    }
}
