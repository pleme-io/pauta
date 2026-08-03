;; pauta's authored action vocabulary — the (defpautaaction) surface.
;;
;; This file IS the keymap, the legend and the --help. Before it existed the
;; bindings were a Rust table with a test holding it to `Action::ALL`; that
;; caught a missing binding but could not stop the legend and the runtime
;; drifting apart, which is exactly what happened to banken (its hand-written
;; legend advertised `S` for a chord bound as `shift+s`).
;;
;; `:binding` is what the operator presses. `:effect` is what the runtime
;; does. `:gate` is the load-bearing field: `binding` means the action
;; changes a pull request's mergeability and therefore MUST be staged and
;; confirmed before it reaches the forge. A `(defpautaaction)` cannot author
;; an unconfirmed binding action — there is no value to author it into.

;; ── Navigation. Never gated: moving a cursor changes nothing. ──

(defpautaaction :name "select-next" :binding "j" :effect select-next  :gate free)
(defpautaaction :name "select-prev" :binding "k" :effect select-prev  :gate free)

;; ── Reading. Also free — every one of these is an OBSERVE. ──

(defpautaaction :name "open-diff"        :binding "enter" :effect open-diff        :gate free)
(defpautaaction :name "close-diff"       :binding "esc"   :effect close-diff       :gate free)
(defpautaaction :name "toggle-diff-mode" :binding "s"     :effect toggle-diff-mode :gate free)
(defpautaaction :name "cycle-lane"       :binding "tab"   :effect cycle-lane       :gate free)

;; ── The verdicts. BINDING: each notifies the author and changes whether
;;    the PR can merge, so each stages a preview and waits for a confirm. ──

(defpautaaction :name "approve"         :binding "a" :effect approve         :gate binding)
(defpautaaction :name "request-changes" :binding "r" :effect request-changes :gate binding)

;; ── The gate's own two keys. `confirm` commits whatever is staged; with
;;    nothing staged it is a no-op, never an invented verdict. ──

(defpautaaction :name "confirm" :binding "y" :effect confirm :gate free)
(defpautaaction :name "cancel"  :binding "n" :effect cancel  :gate free)

(defpautaaction :name "quit" :binding "q" :effect quit :gate free)
