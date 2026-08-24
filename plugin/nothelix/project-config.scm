;;; project-config.scm — Per-project display config (`.nothelix.conf`).
;;;
;;; Auto-discovered by walking up from the open notebook's directory to the
;;; filesystem root. Applies ONLY display settings — font sizes, math colour,
;;; render width, conceal-on-open — never anything executable. Opening an
;;; untrusted repo can therefore change at most how things *look*, never what
;;; runs.
;;;
;;; Format — a flat `key = value` file at the project root. Blank lines and
;;; lines beginning with `#` or `;` are comments. A missing file is a no-op:
;;;
;;;   # nothelix project config (display-only)
;;;   render-cache     = true
;;;
;;; The authoritative key list lives in *config-key-specs* below and is what
;;; `:nothelix-config-keys` prints; unknown keys warn on open instead of being
;;; silently ignored.
;;;
;;; A line-based format (not s-expr) is deliberate: Steel's `read` leaves the
;;; reader wedged after a parse error, so one malformed config could silently
;;; break parsing for later notebooks. `string-split` cannot wedge — a bad line
;;; is simply skipped.

(require "string-utils.scm")
(require "debug.scm")
(require "math-image.scm")
(require "table-image.scm")
(require "image-cache.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          path-exists
                          read-file-tail
                          getenv
                          nothelix-trust-contains
                          nothelix-trust-add
                          nothelix-trust-remove))

(provide conceal-on-open?
         plot-mode
         set-plot-mode!
         picker-jump
         set-picker-jump!
         slm-summaries?
         set-slm-summaries!
         audio-autoplay?
         set-audio-autoplay!
         run-interrupts?
         set-run-interrupts!
         audio-refresh-ms
         set-audio-refresh-ms!
         audio-window-ms
         set-audio-window-ms!
         audio-waveform-rows
         set-audio-waveform-rows!
         audio-view
         set-audio-view!
         audio-seek-ladder
         set-audio-seek-ladder!
         audio-accel-window-ms
         set-audio-accel-window-ms!
         audio-sweep-ms
         set-audio-sweep-ms!
         widgets-enabled?
         set-widgets-enabled!
         config-key-specs
         config-keys-reference
         unknown-config-keys
         maybe-apply-project-config!
         apply-project-config!
         find-project-config
         parse-project-config
         config-ref
         ;; trust + executable-runtime surface (display config is above)
         project-dir-for
         project-trusted?
         trust-project!
         untrust-project!
         project-runtime-for
         executable-fields-present?
         focused-notebook-path)

(define *config-filename* ".nothelix.conf")

;; conceal-on-open defaults to on; a project file may turn it off.
(define *conceal-on-open* (box #true))
(define (conceal-on-open?) (unbox *conceal-on-open*))

;;@doc
;; Opt-in on-device SLM cell summaries for the picker. Defaults off —
;; a project file must explicitly set `slm-summaries = true`. Display-class
;; config: the SLM helper is always compiled from the repo-vendored Swift
;; source, never an arbitrary executable, so this needs no trust gate.
(define *slm-summaries* (box #false))
(define (slm-summaries?) (unbox *slm-summaries*))

;;@doc
;; Override the SLM-summaries opt-in. Ignores anything that isn't a
;; boolean, same guard `apply-project-config!` applies to a config value.
(define (set-slm-summaries! v)
  (when (boolean? v) (set-box! *slm-summaries* v)))

;;@doc
;; How a UnicodePlots-capable cell should render: "auto" (kernel decides,
;; today always braille for a UnicodePlots value), "raster" (force a PNG
;; capture instead of braille text), or "braille" (force the colored
;; braille text-plot path). Threaded into the kernel's execute-cell
;; request (see `execution.scm`'s `kernel-execute-cell-start` calls); the
;; kernel does not yet branch on it — see `libnothelix/src/kernel.rs`'s
;; `kernel_execute_cell_start` doc comment for the follow-up note.
(define *plot-mode* (box "auto"))
(define (plot-mode) (unbox *plot-mode*))

(define (valid-plot-mode? v)
  (or (equal? v "auto") (equal? v "raster") (equal? v "braille")))

;;@doc
;; Override the plot-render mode. Ignores anything that isn't one of
;; "auto"/"raster"/"braille" — an untrusted or malformed config value
;; leaves the current mode (default "auto") in place.
(define (set-plot-mode! v)
  (when (and (string? v) (valid-plot-mode? v)) (set-box! *plot-mode* v)))

(define *picker-jump* (box 10))
(define (picker-jump) (unbox *picker-jump*))
(define (set-picker-jump! v)
  (when (and (exact-integer? v) (> v 0) (<= v 100)) (set-box! *picker-jump* v)))

;;@doc
;; Whether a cell that produced audio auto-plays it on run. Defaults on;
;; `audio-autoplay = false` in a project config suppresses the base layer's
;; auto-play so a clip only sounds on an explicit `<space>ns`.
(define *audio-autoplay* (box #true))
(define (audio-autoplay?) (unbox *audio-autoplay*))
(define (set-audio-autoplay! v)
  (when (boolean? v) (set-box! *audio-autoplay* v)))

;;@doc
;; Milliseconds between redraws of a playing clip's graph. Lower moves more
;; smoothly and costs more redraws; clamped to a sane 40..1000 band.
(define *audio-refresh-ms* (box 100))
(define (audio-refresh-ms) (unbox *audio-refresh-ms*))
(define (set-audio-refresh-ms! v)
  (when (and (exact-integer? v) (>= v 40) (<= v 1000))
    (set-box! *audio-refresh-ms* v)))

;;@doc
;; Milliseconds of signal shown around the playing position. The default is
;; short enough that individual cycles resolve, so the trace reads as a wave
;; rather than a block of peaks; widen it to trade wave detail for context.
;; Clamped to 5..30000.
(define *audio-window-ms* (box 40))
(define (audio-window-ms) (unbox *audio-window-ms*))
(define (set-audio-window-ms! v)
  (when (and (exact-integer? v) (>= v 5) (<= v 30000))
    (set-box! *audio-window-ms* v)))

;;@doc
;; `run-interrupts = false` makes the run key refuse on a running cell
;; instead of interrupting it. Default on: run on the running cell stops it.
(define *run-interrupts* (box #true))
(define (run-interrupts?) (unbox *run-interrupts*))
(define (set-run-interrupts! v)
  (when (boolean? v) (set-box! *run-interrupts* v)))

;;@doc
;; Braille rows drawn for a cell's waveform under its output. Defaults to 4;
;; a project config may widen or narrow it within a sane 1..16 band.
(define *audio-waveform-rows* (box 4))
(define (audio-waveform-rows) (unbox *audio-waveform-rows*))
(define (set-audio-waveform-rows! v)
  (when (and (exact-integer? v) (> v 0) (<= v 16)) (set-box! *audio-waveform-rows* v)))

;;@doc
;; Which view a cell's audio renders under its output: "waveform" (the braille
;; envelope, default) or "spectrogram" (the STFT heatmap). The scrub modal's
;; `v` key flips its own live view without touching this knob, so reopening a
;; clip always follows the configured default.
(define *audio-view* (box "waveform"))
(define (audio-view) (unbox *audio-view*))

(define (valid-audio-view? v)
  (or (equal? v "waveform") (equal? v "spectrogram")))

;;@doc
;; Override the audio view. Ignores anything that isn't "waveform" or
;; "spectrogram", so a malformed config value leaves the current view in place.
(define (set-audio-view! v)
  (when (and (string? v) (valid-audio-view? v)) (set-box! *audio-view* v)))

;;@doc
;; The seek-step ladder in milliseconds, coarsest last. Quick seeks and scrub
;; mode escalate/step along it. Defaults to (100 500 1000 5000 30000).
(define *audio-seek-ladder* (box (list 100 500 1000 5000 30000)))
(define (audio-seek-ladder) (unbox *audio-seek-ladder*))
(define (set-audio-seek-ladder! v)
  (when (and (list? v) (not (null? v)) (all-positive-numbers? v))
    (set-box! *audio-seek-ladder* v)))

;;@doc
;; Window in milliseconds within which repeated quick seeks escalate the step
;; along the ladder; a longer gap resets to the base step. Defaults to 700.
(define *audio-accel-window-ms* (box 700))
(define (audio-accel-window-ms) (unbox *audio-accel-window-ms*))
(define (set-audio-accel-window-ms! v)
  (when (and (exact-integer? v) (> v 0)) (set-box! *audio-accel-window-ms* v)))

;;@doc
;; Duration in milliseconds of the scrub-mode playhead sweep between the old
;; and new column, held constant regardless of seek distance. Defaults to 100.
(define *audio-sweep-ms* (box 100))
(define (audio-sweep-ms) (unbox *audio-sweep-ms*))
(define (set-audio-sweep-ms! v)
  (when (and (exact-integer? v) (> v 0)) (set-box! *audio-sweep-ms* v)))

;;@doc
;; Whether the unified widget surfaces — the ]w/[w walk and the shared modal
;; shell — are active. Defaults on; `widgets = false` in a project config turns
;; them off, leaving the pre-widget feature keys (]p/[p, ]a/[a, plot/animation
;; commands) untouched.
(define *widgets-enabled* (box #true))
(define (widgets-enabled?) (unbox *widgets-enabled*))
(define (set-widgets-enabled! v)
  (when (boolean? v) (set-box! *widgets-enabled* v)))

(define (all-positive-numbers? lst)
  (cond
    [(null? lst) #true]
    [(and (number? (car lst)) (> (car lst) 0)) (all-positive-numbers? (cdr lst))]
    [else #false]))

;;@doc
;; Parse a comma-separated "100,500,1000" config value into a list of positive
;; numbers, or #false when any field is missing/non-numeric/non-positive.
(define (parse-seek-ladder-value v)
  (if (not (string? v))
      #false
      (let ([fields (map string->number (string-split v ","))])
        (if (and (not (null? fields)) (all-positive-numbers? fields)) fields #false))))

;; --- path helpers (string-only; no new primitives) ---

;; Directory portion of a path: everything before the last "/".
(define (path-dir p)
  (let ([parts (string-split p "/")])
    (if (<= (length parts) 1)
        ""
        (string-join (reverse (cdr (reverse parts))) "/"))))

;; Walk up from `dir` looking for the config file; #false if none to the root.
(define (find-config-in-or-above dir)
  (let ([candidate (string-append dir "/" *config-filename*)])
    (cond
      [(equal? (path-exists candidate) "yes") candidate]
      [(or (equal? dir "") (equal? dir "/")) #false]
      [else
       (let ([parent (path-dir dir)])
         (if (equal? parent dir)
             #false
             (find-config-in-or-above parent)))])))

;;@doc
;; Path to the `.nothelix.conf` governing `notebook-path`, or #false.
(define (find-project-config notebook-path)
  (and notebook-path
       (find-config-in-or-above (path-dir notebook-path))))

;; --- parsing ---

;; "16" -> 16, "true"/"false" -> bool, anything else (e.g. "#d0d0d0") -> string.
(define (coerce-value v)
  (cond
    [(equal? v "true") #true]
    [(equal? v "false") #false]
    [else (let ([n (string->number v)]) (if n n v))]))

;; One line -> (key . value) pair, or #false to skip (blank / comment / junk).
(define (parse-config-line line)
  (let ([t (string-trim line)])
    (cond
      [(= (string-length t) 0) #false]
      [(char=? (string-ref t 0) #\;) #false]
      [(char=? (string-ref t 0) #\#) #false]
      [else
       (let ([parts (string-split t "=")])
         (if (>= (length parts) 2)
             (let ([k (string-trim (car parts))]
                   ;; rejoin so a stray '=' in the value survives
                   [v (string-trim (string-join (cdr parts) "="))])
               (if (> (string-length k) 0)
                   (cons k (coerce-value v))
                   #false))
             #false))])))

;;@doc
;; Read the config file at `path` into an alist of (string-key . value). '() on
;; a missing/empty file. Cannot error on malformed content — bad lines skip.
(define (parse-project-config path)
  (let ([contents (read-file-tail path 1000000000)])
    (if (string? contents)
        (let loop ([lines (string-split contents "\n")] [acc '()])
          (if (null? lines)
              (reverse acc)
              (let ([pair (parse-config-line (car lines))])
                (loop (cdr lines) (if pair (cons pair acc) acc)))))
        '())))

;; Lookup `key` in `alist`; `default` when absent. No `assoc` dependency.
(define (config-ref alist key default)
  (let loop ([xs alist])
    (cond
      [(null? xs) default]
      [(and (pair? (car xs)) (equal? (car (car xs)) key)) (cdr (car xs))]
      [else (loop (cdr xs))])))

(define (strip-hash s)
  (if (and (> (string-length s) 0) (char=? (string-ref s 0) #\#))
      (substring s 1 (string-length s))
      s))

;; --- apply ---

(define (config-key-spec name type default doc setter)
  (list name type default doc setter))

(define (spec-name spec) (list-ref spec 0))
(define (spec-type spec) (list-ref spec 1))
(define (spec-default spec) (list-ref spec 2))
(define (spec-doc spec) (list-ref spec 3))
(define (spec-setter spec) (list-ref spec 4))

(define (parse-seek-ladder-field v)
  (if (string? v) (parse-seek-ladder-value v) #false))

(define *config-key-specs*
  (list
    (config-key-spec "conceal-on-open" "boolean" "#true"
      "auto-conceal LaTeX when a file opens"
      (lambda (v) (when (boolean? v) (set-box! *conceal-on-open* v))))
    (config-key-spec "render-cache" "boolean" "#true"
      "persist completed math/table renders to disk and reuse them on reopen"
      set-render-cache!)
    (config-key-spec "math-font-pt" "integer" "14"
      "font size for display-math images"
      set-math-image-font-pt!)
    (config-key-spec "table-font-pt" "integer" "13"
      "font size for table images"
      set-table-image-font-pt!)
    (config-key-spec "math-color" "colour" "theme-derived"
      "pin the math/table ink as rrggbb; disables theme auto-colour"
      (lambda (v) (when (string? v) (set-math-image-color! (strip-hash v)))))
    (config-key-spec "render-width" "integer" "derived from view"
      "pin math/table image width in columns"
      (lambda (v) (when (and (exact-integer? v) (> v 0)) (set-math-image-width-override! v))))
    (config-key-spec "plots-per-cell" "integer" "32"
      "cap on stacked plots rendered per cell, clamped to 1..256"
      set-plots-per-cell!)
    (config-key-spec "plot-mode" "auto|raster|braille" "auto"
      "force a plot rendering path or let the backend decide"
      set-plot-mode!)
    (config-key-spec "picker-jump" "integer" "10"
      "rows the cell navigator scrolls per jump, clamped to 1..100"
      set-picker-jump!)
    (config-key-spec "slm-summaries" "boolean" "#false"
      "label picker rows with Apple's on-device model (macOS 26+, opt-in)"
      set-slm-summaries!)
    (config-key-spec "audio-autoplay" "boolean" "#true"
      "auto-play a cell's audio when it runs"
      set-audio-autoplay!)
    (config-key-spec "audio-view" "waveform|spectrogram" "waveform"
      "which view a cell's audio renders under its output"
      set-audio-view!)
    (config-key-spec "audio-waveform-rows" "integer" "4"
      "braille rows drawn for a playing clip, clamped to 1..16"
      set-audio-waveform-rows!)
    (config-key-spec "audio-refresh-ms" "integer" "100"
      "milliseconds between redraws of a playing clip's graph, clamped to 40..1000"
      set-audio-refresh-ms!)
    (config-key-spec "audio-window-ms" "integer" "40"
      "milliseconds of signal shown around the playing position, clamped to 5..30000"
      set-audio-window-ms!)
    (config-key-spec "audio-seek-ladder" "comma-numbers" "100,500,1000,5000,30000"
      "seek-step ladder in milliseconds, coarsest last"
      (lambda (v) (let ([ladder (parse-seek-ladder-field v)]) (when ladder (set-audio-seek-ladder! ladder)))))
    (config-key-spec "audio-accel-window-ms" "integer" "700"
      "window in which repeated seeks escalate along the ladder"
      set-audio-accel-window-ms!)
    (config-key-spec "audio-sweep-ms" "integer" "100"
      "duration of the scrub-mode playhead sweep between columns"
      set-audio-sweep-ms!)
    (config-key-spec "widgets" "boolean" "#true"
      "enable the ]w/[w widget walk and shared modal surfaces"
      set-widgets-enabled!)
    (config-key-spec "run-interrupts" "boolean" "#true"
      "whether running a running cell interrupts it instead of refusing"
      set-run-interrupts!)))

;;@doc
;; Every supported `.nothelix.conf` key as a spec list of
;; (name type default doc setter). Single source of truth for parsing,
;; validation warnings, and :nothelix-config-keys.
(define (config-key-specs) *config-key-specs*)

(define *executable-config-keys* '("julia-bin" "julia-project"))

(define (known-config-key? name)
  (or (member name *executable-config-keys*)
      (let find ([specs *config-key-specs*])
        (cond
          [(null? specs) #false]
          [(equal? (spec-name (car specs)) name) #true]
          [else (find (cdr specs))]))))

;;@doc
;; Keys present in a parsed config that nothelix does not know, in file order.
(define (unknown-config-keys alist)
  (let loop ([xs alist] [seen '()] [acc '()])
    (if (null? xs)
        (reverse acc)
        (let ([name (car (car xs))])
          (loop (cdr xs)
                (cons name seen)
                (if (and (not (member name seen)) (not (known-config-key? name)))
                    (cons name acc)
                    acc))))))

;;@doc
;; Multi-line reference of every config key: name, type, default, and effect,
;; formatted for the status line and helix.log.
(define (config-keys-reference)
  (string-join
    (map (lambda (spec)
           (string-append (spec-name spec)
                          "  [" (spec-type spec) "]"
                          "  default: " (spec-default spec)
                          "  — " (spec-doc spec)))
         *config-key-specs*)
    "\n"))

;;@doc
;; Apply a parsed config alist to the live display state. Every setter
;; type-checks its own input, so a malformed value is skipped rather than
;; corrupting state. Returns the list of unknown keys that were skipped.
(define (apply-project-config! alist)
  (for-each
    (lambda (spec)
      (let ([v (config-ref alist (spec-name spec) 'unset)])
        (when (not (eq? v 'unset))
          ((spec-setter spec) v))))
    *config-key-specs*)
  (unknown-config-keys alist))

;; --- executable runtime (trust-gated) ---
;;
;; `julia-bin` / `julia-project` are EXECUTABLE settings: a project that sets
;; them gets its own interpreter/env launched. They are applied ONLY for a
;; project directory the user has explicitly trusted (allowlist in the dylib,
;; canonicalized paths). Untrusted -> the kernel falls back to PATH julia with
;; the default env, exactly as if the fields were absent.

;;@doc
;; Absolute path of the directory that owns `notebook-path`'s config, or #false.
(define (project-dir-for notebook-path)
  (let ([cfg (find-project-config notebook-path)])
    (and cfg (path-dir cfg))))

;;@doc
;; #true if `dir` is on the trust allowlist.
(define (project-trusted? dir)
  (and dir (equal? (nothelix-trust-contains dir) "yes")))

;;@doc
;; Add `dir` to the trust allowlist. "" on success, "ERROR: …" otherwise.
(define (trust-project! dir) (nothelix-trust-add dir))

;;@doc
;; Remove `dir` from the trust allowlist. "" on success, "ERROR: …" otherwise.
(define (untrust-project! dir) (nothelix-trust-remove dir))

;; Raw (julia-bin . julia-project) strings from a parsed alist; "" when absent.
(define (config-executable-fields alist)
  (let ([b (config-ref alist "julia-bin" "")]
        [p (config-ref alist "julia-project" "")])
    (cons (if (string? b) b "")
          (if (string? p) p ""))))

;;@doc
;; #true if a parsed config requests any executable runtime field.
(define (executable-fields-present? alist)
  (let ([ef (config-executable-fields alist)])
    (or (> (string-length (car ef)) 0)
        (> (string-length (cdr ef)) 0))))

(define (expand-home v)
  (if (and (> (string-length v) 0) (char=? (string-ref v 0) #\~))
      (string-append (getenv "HOME") (substring v 1 (string-length v)))
      v))

;; Drop a leading "./" so a relative value joins cleanly (no "dir/./x").
(define (strip-dot-slash v)
  (if (and (>= (string-length v) 2)
           (char=? (string-ref v 0) #\.)
           (char=? (string-ref v 1) #\/))
      (substring v 2 (string-length v))
      v))

;; Resolve a config-supplied path: absolute as-is, ~ expanded, else relative to
;; the project directory.
(define (resolve-against-dir dir v)
  (cond
    [(equal? v "") ""]
    [(char=? (string-ref v 0) #\/) v]
    [(char=? (string-ref v 0) #\~) (expand-home v)]
    [else (string-append dir "/" (strip-dot-slash v))]))

;;@doc
;; The (julia-bin . julia-project) a notebook's project requests — but ONLY if
;; the project directory is trusted. Untrusted / unconfigured -> ("" . "").
;; This is the secure point-of-use check the kernel calls at spawn time.
(define (project-runtime-for notebook-path)
  (let ([cfg (find-project-config notebook-path)])
    (if (not cfg)
        (cons "" "")
        (let ([dir (path-dir cfg)])
          (if (not (project-trusted? dir))
              (cons "" "")
              (let* ([alist (parse-project-config cfg)]
                     [ef (config-executable-fields alist)])
                (cons (resolve-against-dir dir (car ef))
                      (resolve-against-dir dir (cdr ef)))))))))

;; --- orchestration ---

;;@doc
;; Path of the focused document, or #false.
(define (focused-notebook-path)
  (let ([doc-id (editor->doc-id (editor-focus))])
    (and doc-id (editor-document->path doc-id))))

;;@doc
;; Find, read, and apply the project config for the focused notebook (if any).
;; Display settings always apply; executable settings only hint the user toward
;; :nothelix-trust-project until the project directory is trusted.
(define (maybe-apply-project-config!)
  (let ([path (focused-notebook-path)])
    (when path
      (let ([cfg-path (find-project-config path)])
        (when cfg-path
          (let* ([alist (parse-project-config cfg-path)]
                 [dir (path-dir cfg-path)]
                 [unknown (apply-project-config! alist)]
                 [untrusted?
                  (and (executable-fields-present? alist)
                       (not (project-trusted? dir)))])
            (when (> (length unknown) 0)
              (debug-log
                (string-append "project-config: ignored unknown keys in " cfg-path ": "
                               (string-join unknown ", "))))
            (cond
              [untrusted?
               (set-status!
                 (string-append "nothelix: " dir
                                " requests a custom Julia runtime — run :nothelix-trust-project to allow"))]
              [(> (length unknown) 0)
               (set-status!
                 (string-append "nothelix: applied " cfg-path
                                " — ignored unknown key(s): "
                                (string-join unknown ", ")))]
              [else
               (set-status! (string-append "nothelix: applied " cfg-path))])))))))
