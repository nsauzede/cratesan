- state : full or partial game state
map : empty if partial
moves, pushes
stored
time_s
px, py

- undos : stack of incremental states between moves

- snapshots : stack of full arbitrary states + undo stack
state State
undos_states [] State
undos : total number of undo_pop calls
