// UIEvents WebIDL — W3C UI Events + Pointer Events spec
// https://www.w3.org/TR/uievents/
// https://w3c.github.io/pointerevents/

// ── EventModifierInit (shared by Mouse/Keyboard) ──

dictionary EventModifierInit : UIEventInit {
  boolean ctrlKey = false;
  boolean shiftKey = false;
  boolean altKey = false;
  boolean metaKey = false;
  boolean modifierAltGraph = false;
  boolean modifierCapsLock = false;
  boolean modifierFn = false;
  boolean modifierFnLock = false;
  boolean modifierHyper = false;
  boolean modifierNumLock = false;
  boolean modifierScrollLock = false;
  boolean modifierSuper = false;
  boolean modifierSymbol = false;
  boolean modifierSymbolLock = false;
};

// ── UIEvent ──

[Exposed=Window]
interface UIEvent : Event {
  constructor(DOMString type, optional UIEventInit eventInitDict = {});
  readonly attribute Window? view;
  readonly attribute long detail;
  undefined initUIEvent(DOMString typeArg, optional boolean bubblesArg = false, optional boolean cancelableArg = false, optional Window? viewArg = null, optional long detailArg = 0);
};

dictionary UIEventInit : EventInit {
  Window? view = null;
  long detail = 0;
};

// ── FocusEvent ──

[Exposed=Window]
interface FocusEvent : UIEvent {
  constructor(DOMString type, optional FocusEventInit eventInitDict = {});
  readonly attribute EventTarget? relatedTarget;
};

dictionary FocusEventInit : UIEventInit {
  EventTarget? relatedTarget = null;
};

// ── MouseEvent ──

[Exposed=Window]
interface MouseEvent : UIEvent {
  constructor(DOMString type, optional MouseEventInit eventInitDict = {});
  readonly attribute long screenX;
  readonly attribute long screenY;
  readonly attribute long clientX;
  readonly attribute long clientY;
  readonly attribute long layerX;
  readonly attribute long layerY;
  readonly attribute long pageX;
  readonly attribute long pageY;
  readonly attribute long x;
  readonly attribute long y;
  readonly attribute long offsetX;
  readonly attribute long offsetY;
  readonly attribute long movementX;
  readonly attribute long movementY;
  readonly attribute boolean ctrlKey;
  readonly attribute boolean shiftKey;
  readonly attribute boolean altKey;
  readonly attribute boolean metaKey;
  readonly attribute short button;
  readonly attribute unsigned short buttons;
  readonly attribute EventTarget? relatedTarget;
  boolean getModifierState(DOMString keyArg);
  undefined initMouseEvent(DOMString typeArg, optional boolean bubblesArg = false, optional boolean cancelableArg = false, optional Window? viewArg = null, optional long detailArg = 0, optional long screenXArg = 0, optional long screenYArg = 0, optional long clientXArg = 0, optional long clientYArg = 0, optional boolean ctrlKeyArg = false, optional boolean altKeyArg = false, optional boolean shiftKeyArg = false, optional boolean metaKeyArg = false, optional short buttonArg = 0, optional EventTarget? relatedTargetArg = null);
};

dictionary MouseEventInit : EventModifierInit {
  long screenX = 0;
  long screenY = 0;
  long clientX = 0;
  long clientY = 0;
  short button = 0;
  unsigned short buttons = 0;
  EventTarget? relatedTarget = null;
};

// ── WheelEvent ──

[Exposed=Window]
interface WheelEvent : MouseEvent {
  constructor(DOMString type, optional WheelEventInit eventInitDict = {});
  const unsigned long DOM_DELTA_PIXEL = 0x00;
  const unsigned long DOM_DELTA_LINE = 0x01;
  const unsigned long DOM_DELTA_PAGE = 0x02;
  readonly attribute double deltaX;
  readonly attribute double deltaY;
  readonly attribute double deltaZ;
  readonly attribute unsigned long deltaMode;
};

dictionary WheelEventInit : MouseEventInit {
  double deltaX = 0.0;
  double deltaY = 0.0;
  double deltaZ = 0.0;
  unsigned long deltaMode = 0;
};

// ── KeyboardEvent ──

[Exposed=Window]
interface KeyboardEvent : UIEvent {
  constructor(DOMString type, optional KeyboardEventInit eventInitDict = {});
  const unsigned long DOM_KEY_LOCATION_STANDARD = 0x00;
  const unsigned long DOM_KEY_LOCATION_LEFT = 0x01;
  const unsigned long DOM_KEY_LOCATION_RIGHT = 0x02;
  const unsigned long DOM_KEY_LOCATION_NUMPAD = 0x03;
  readonly attribute DOMString key;
  readonly attribute DOMString code;
  readonly attribute unsigned long location;
  readonly attribute boolean ctrlKey;
  readonly attribute boolean shiftKey;
  readonly attribute boolean altKey;
  readonly attribute boolean metaKey;
  readonly attribute boolean repeat;
  readonly attribute boolean isComposing;
  readonly attribute unsigned long charCode;
  readonly attribute unsigned long keyCode;
  readonly attribute unsigned long which;
  boolean getModifierState(DOMString keyArg);
  undefined initKeyboardEvent(DOMString typeArg, optional boolean bubblesArg = false, optional boolean cancelableArg = false, optional Window? viewArg = null, optional DOMString keyArg = "", optional unsigned long locationArg = 0, optional boolean ctrlKeyArg = false, optional boolean altKeyArg = false, optional boolean shiftKeyArg = false, optional boolean metaKeyArg = false);
};

dictionary KeyboardEventInit : EventModifierInit {
  DOMString key = "";
  DOMString code = "";
  unsigned long location = 0;
  boolean repeat = false;
  boolean isComposing = false;
  unsigned long charCode = 0;
  unsigned long keyCode = 0;
  unsigned long which = 0;
};

// ── InputEvent ──

[Exposed=Window]
interface InputEvent : UIEvent {
  constructor(DOMString type, optional InputEventInit eventInitDict = {});
  readonly attribute DOMString? data;
  readonly attribute boolean isComposing;
  readonly attribute DOMString inputType;
  readonly attribute DataTransfer? dataTransfer;
};

dictionary InputEventInit : UIEventInit {
  DOMString? data = null;
  boolean isComposing = false;
  DOMString inputType = "";
  DataTransfer? dataTransfer = null;
};

// ── CompositionEvent ──

[Exposed=Window]
interface CompositionEvent : UIEvent {
  constructor(DOMString type, optional CompositionEventInit eventInitDict = {});
  readonly attribute DOMString data;
  undefined initCompositionEvent(DOMString typeArg, optional boolean bubblesArg = false, optional boolean cancelableArg = false, optional Window? viewArg = null, optional DOMString dataArg = "");
};

dictionary CompositionEventInit : UIEventInit {
  DOMString data = "";
};

// ── PointerEvent ──

[Exposed=Window]
interface PointerEvent : MouseEvent {
  constructor(DOMString type, optional PointerEventInit eventInitDict = {});
  readonly attribute long pointerId;
  readonly attribute double width;
  readonly attribute double height;
  readonly attribute float pressure;
  readonly attribute float tangentialPressure;
  readonly attribute long tiltX;
  readonly attribute long tiltY;
  readonly attribute long twist;
  readonly attribute double altitudeAngle;
  readonly attribute double azimuthAngle;
  readonly attribute DOMString pointerType;
  readonly attribute boolean isPrimary;
  sequence<PointerEvent> getCoalescedEvents();
  sequence<PointerEvent> getPredictedEvents();
};

dictionary PointerEventInit : MouseEventInit {
  long pointerId = 0;
  double width = 1;
  double height = 1;
  float pressure = 0;
  float tangentialPressure = 0;
  long tiltX = 0;
  long tiltY = 0;
  long twist = 0;
  double altitudeAngle = 0;
  double azimuthAngle = 0;
  DOMString pointerType = "";
  boolean isPrimary = false;
};
