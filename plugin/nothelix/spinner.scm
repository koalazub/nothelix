;;; spinner.scm - Loading spinner animations for cell execution

(provide spinner-frames
         spinner-next-frame
         spinner-reset
         *spinner-frame*)

;; Helix structure spinner - represents a rotating double helix/spring
;; Each frame shows the helix at a different rotation angle
;; Uses Braille patterns to create a visual helix structure
(define spinner-frames
  (vector "⠿⠶⠿⠶⠿"  ;; Helix rotation 0°   - top strand forward, bottom back
          "⠾⠷⠾⠷⠾"  ;; Helix rotation 30°  - strands rotating
          "⠼⠧⠼⠧⠼"  ;; Helix rotation 60°  - strands crossing
          "⠸⠏⠸⠏⠸"  ;; Helix rotation 90°  - top strand back, bottom forward
          "⠴⠋⠴⠋⠴"  ;; Helix rotation 120° - strands rotating back
          "⠦⠙⠦⠙⠦"  ;; Helix rotation 150° - strands crossing back
          "⠧⠹⠧⠹⠧"  ;; Helix rotation 180° - opposite of 0°
          "⠇⠸⠇⠸⠇"  ;; Helix rotation 210° - continuing rotation
          "⠏⠼⠏⠼⠏"  ;; Helix rotation 240° - strands crossing
          "⠋⠴⠋⠴⠋"  ;; Helix rotation 270° - perpendicular view
          "⠙⠦⠙⠦⠙"  ;; Helix rotation 300° - approaching start
          "⠹⠧⠹⠧⠹")) ;; Helix rotation 330° - almost back to start

;; Current spinner frame index
(define *spinner-frame* 0)

;; Get the next spinner frame and advance the counter
(define (spinner-next-frame)
  (define frame (vector-ref spinner-frames *spinner-frame*))
  (set! *spinner-frame* (modulo (+ *spinner-frame* 1) (vector-length spinner-frames)))
  frame)

;; Reset spinner to first frame
(define (spinner-reset)
  (set! *spinner-frame* 0))
