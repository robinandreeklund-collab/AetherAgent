// HTML Media Elements — from HTML Living Standard
// Behövs för html/semantics audio/video tester

interface HTMLMediaElement : HTMLElement {
  attribute DOMString src;
  attribute DOMString currentSrc;
  attribute DOMString crossOrigin;
  readonly attribute unsigned short networkState;
  attribute DOMString preload;
  readonly attribute unsigned short readyState;
  readonly attribute boolean seeking;
  attribute double currentTime;
  readonly attribute double duration;
  readonly attribute boolean paused;
  attribute double defaultPlaybackRate;
  attribute double playbackRate;
  readonly attribute boolean ended;
  attribute boolean autoplay;
  attribute boolean loop;
  attribute boolean controls;
  attribute double volume;
  attribute boolean muted;
  attribute boolean defaultMuted;

  void play();
  void pause();
  void load();
  DOMString canPlayType(DOMString type);
  void fastSeek(double time);
};

interface HTMLVideoElement : HTMLMediaElement {
  attribute unsigned long width;
  attribute unsigned long height;
  readonly attribute unsigned long videoWidth;
  readonly attribute unsigned long videoHeight;
  attribute DOMString poster;
  attribute boolean playsInline;
};

interface HTMLAudioElement : HTMLMediaElement {
};

interface HTMLSourceElement : HTMLElement {
  attribute DOMString src;
  attribute DOMString type;
  attribute DOMString srcset;
  attribute DOMString sizes;
  attribute DOMString media;
  attribute unsigned long width;
  attribute unsigned long height;
};

interface HTMLTrackElement : HTMLElement {
  attribute DOMString kind;
  attribute DOMString src;
  attribute DOMString srclang;
  attribute DOMString label;
  attribute boolean default;
  readonly attribute unsigned short readyState;
};
