extern crate sdl2;

use sdl2::event::Event;
use sdl2::image::{InitFlag, LoadTexture};
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use std::env::current_exe;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

const TITLE: &str = "クレートさん Rust";
const VERSION: u8 = 1;
const ZOOM: usize = 2;
const TEXT_SIZE: usize = 8;
const TEXT_RATIO: usize = ZOOM;
const WIDTH: usize = 320 * ZOOM;
const HEIGHT: usize = 200 * ZOOM;
const EMPTY: u8 = 0x0;
const STORE: u8 = 0x1;
const CRATE: u8 = 0x2;
const WALL: u8 = 0x4;
const C_EMPTY: char = ' ';
const C_STORE: char = '.';
const C_STORED: char = '*';
const C_CRATE: char = '$';
const C_PLAYER: char = '@';
const C_SPLAYER: char = '&';
const C_WALL: char = '#';
const FONT_FILE: &str = "RobotoMono-Regular.ttf";
const LEVELS_FILE: &str = "levels.txt";
const SCORES_FILE: &str = "scores.txt";
const I_EMPTY: &str = "empty.png";
const I_STORE: &str = "store.png";
const I_STORED: &str = "stored.png";
const I_CRATE: &str = "crate.png";
const I_PLAYER: &str = "player.png";
const I_SPLAYER: &str = "splayer.png";
const I_WALL: &str = "wall.png";
const N_EMPTY: usize = 0;
const N_STORE: usize = 1;
const N_STORED: usize = 2;
const N_CRATE: usize = 3;
const N_PLAYER: usize = 4;
const N_SPLAYER: usize = 5;
const N_WALL: usize = 6;
const N_WHITE: usize = 7;

type Map = Vec<Vec<u8>>;

enum Status {
	Play,
	Pause,
	Win,
}

struct Level {
	crates: u32,
	w: usize,
	h: usize,
	map: Map,
	stored: u32,
	px: usize,
	py: usize,
}

#[derive(Debug)]
struct Score {
	level: u16,
	moves: u16,
	pushes: u16,
	time_s: u32,
}

struct State {
	map: Map,
	moves: i32,
	pushes: i32,
	time_s: u32,
	stored: u32,
	px: usize,
	py: usize,
	undos: u32,
}

struct Game<'ttf> {
	// Game flags and status
	quit: bool,
	status: Status,
	must_draw: bool,
	debug: bool,
	// Game levels
	levels: Vec<Level>,
	level: usize,
	// Game states
	snapshots: Vec<u8>,
	state: State,
	undo_states: Vec<State>,
	last_ticks: SystemTime,
	scores: Vec<Score>,
	scores_file: String,
	// SDL stuff
	width: usize,
	height: usize,
	bw: usize, // block dims
	bh: usize, // block dims
	// TTF stuff
	font: sdl2::ttf::Font<'ttf, 'static>,
}

impl<'ttf> Game<'ttf> {
	fn debug_dump(&self) {
		if self.debug {
			println!(
				"level={} crates={}/{} moves={} pushes={} undos={}/{} snaps={} time={}",
				self.level + 1,
				self.state.moves,
				self.state.pushes,
				self.state.time_s,
				self.state.stored,
				self.levels[self.level].crates,
				self.snapshots.len(),
				self.state.undos,
				self.undo_states.len(),
			);
		}
	}

	fn save_state(&self, state: &mut State, full: bool) {
		*state = State {
			map: Vec::new(),
			stored: self.state.stored,
			px: self.state.px,
			py: self.state.py,
			time_s: self.state.time_s,
			pushes: self.state.pushes,
			moves: self.state.moves,
			undos: self.state.undos,
		};
		if full {
			state.map = self.state.map.clone()
		}
	}

	fn restore_state(&mut self, state_: State) {
		let map = self.state.map.clone();
		self.state = state_;
		if self.state.map.is_empty() {
			self.state.map = map;
		}
	}

	fn save_score(&mut self) {
		let mut push_score = true;
		for score in &self.scores {
			if score.level as usize == self.level {
				push_score = false;
			}
		}
		if push_score {
			self.scores.push(Score {
				level: self.level as u16,
				pushes: self.state.pushes as u16,
				moves: self.state.moves as u16,
				time_s: self.state.time_s,
			});
		}
	}

	fn save_scores(&self) {
		if !self.scores.is_empty() {
			let mut file = File::create(&self.scores_file).unwrap();
			writeln!(file, "{}", VERSION).unwrap();
			writeln!(file, "{}", self.scores.len()).unwrap();
			for s in &self.scores {
				writeln!(file, "{} {} {} {}", s.level, s.pushes, s.moves, s.time_s).unwrap();
			}
		}
	}

	fn load_scores(scores_file: &str) -> Vec<Score> {
		macro_rules! scan {
			($string:expr, $sep:expr, $( $x:ty ),+ ) => {{
				let mut iter = $string.split($sep);
				($(iter.next().and_then(|word| word.parse::<$x>().ok()).unwrap(),)*)
			}}
		}
		let mut version = 0;
		let mut nscores = 0;
		let mut ret = Vec::new();
		if let Ok(file) = File::open(scores_file) {
			let mut reader = BufReader::new(file);
			let mut line = String::new();
			if let Ok(_nbytes) = reader.read_line(&mut line) {
				version = scan!(line, char::is_whitespace, u8).0;
			}
			if version != VERSION {
				panic!(
					"Invalid scores version. Please delete the scores file {}.",
					&scores_file
				);
			}
			line.clear();
			if let Ok(_nbytes) = reader.read_line(&mut line) {
				nscores = scan!(line, char::is_whitespace, u8).0;
			}
			let mut sscores = String::new();
			reader.read_to_string(&mut sscores).unwrap();
			for line in sscores.lines() {
				let (level, pushes, moves, time_s) =
					scan!(line, char::is_whitespace, u16, u16, u16, u32);
				ret.push(Score {
					level,
					pushes,
					moves,
					time_s,
				});
			}
			if nscores as usize != ret.len() {
				panic!(
					"Invalid number of scores (read {} parsed {}). Please delete the scores file {}.",
					nscores, ret.len(), &scores_file
				);
			}
		}
		ret
	}

	fn pop_undo(&mut self) {
		if let Some(state) = self.undo_states.pop() {
			self.restore_state(state);
			self.save_scores();
			self.state.undos += 1;
			self.debug_dump();
			self.must_draw = true;
		}
	}

	fn push_undo(&mut self, full: bool) {
		let mut s = State {
			map: Vec::new(),
			stored: 0,
			px: 0,
			py: 0,
			time_s: 0,
			pushes: 0,
			moves: 0,
			undos: 0,
		};
		self.save_state(&mut s, full);
		self.undo_states.push(s);
	}

	fn load_levels(levels_file: &str) -> Vec<Level> {
		let mut levels = Vec::new();
		let mut vlevels = Vec::new();
		let mut slevel = String::new();
		let mut level = 1;
		let mut slevels = String::new();
		let mut f = File::open(levels_file)
			.unwrap_or_else(|_| panic!("Couldn't open the levels {}", levels_file));
		f.read_to_string(&mut slevels).unwrap();
		for line in slevels.lines() {
			if line.is_empty() {
				if !slevel.is_empty() {
					vlevels.push(slevel);
					slevel = "".to_string();
				}
				continue;
			}
			if line.starts_with(';') {
				continue;
			}
			slevel = format!("{}\n{}", slevel, line);
		}
		if !slevel.is_empty() {
			vlevels.push(slevel);
		}
		for s in vlevels {
			let mut map = Vec::new();
			let mut crates = 0;
			let mut stores = 0;
			let mut stored = 0;
			let mut w = 0;
			let mut h = 0;
			let mut px = 0;
			let mut py = 0;
			let mut player_found = false;
			for line in s.lines() {
				if line.len() > w {
					w = line.len();
				}
			}
			for line in s.lines() {
				if line.is_empty() {
					continue;
				}
				let mut v = vec![EMPTY; w];
				for (i, e) in line.chars().enumerate() {
					match e {
						C_EMPTY => {
							v[i] = EMPTY;
						}
						C_STORE => {
							v[i] = STORE;
							stores += 1;
						}
						C_CRATE => {
							v[i] = CRATE;
							crates += 1;
						}
						C_STORED => {
							v[i] = CRATE | STORE;
							stores += 1;
							crates += 1;
							stored += 1;
						}
						C_PLAYER => {
							if player_found {
								panic!("Player found multiple times in level {}", level);
							};
							px = i;
							py = h;
							player_found = true;
							v[i] = EMPTY;
						}
						C_SPLAYER => {
							if player_found {
								panic!("Player found multiple times in level {}", level);
							};
							px = i;
							py = h;
							player_found = true;
							v[i] = STORE;
							stores += 1;
						}
						C_WALL => {
							v[i] = WALL;
						}
						_ => {
							panic!("Invalid element [{}] in level", e);
						}
					}
				}
				map.push(v);
				h += 1;
			}
			if crates != stores {
				panic!(
					"Mismatch between crates={} and stores={} in level",
					crates, stores
				);
			}
			if !player_found {
				panic!("Player not found in level {}", level);
			} else {
			}
			levels.push(Level {
				map,
				crates,
				stored,
				w,
				h,
				px,
				py,
			});
			level += 1;
		}
		levels
	}

	fn new(
		ttf_context: &'ttf sdl2::ttf::Sdl2TtfContext,
		root_dir: &std::path::Path,
		width: usize,
		height: usize,
	) -> Game<'ttf> {
		let levels_file = root_dir.join("res").join("levels").join(LEVELS_FILE);
		let levels_file = levels_file.to_str().unwrap();
		let scores_file = root_dir.join(SCORES_FILE).to_str().unwrap().to_string();
		let levels = Game::load_levels(levels_file);
		let scores = Game::load_scores(&scores_file);
		let ttf_file = root_dir.join("res").join("fonts").join(FONT_FILE);
		let font = ttf_context
			.load_font(ttf_file, (TEXT_SIZE * TEXT_RATIO) as u16)
			.expect("Couldn't load the font");

		let mut level = 0;
		let mut dones = vec![false; levels.len()];
		for score in &scores {
			if (score.level as usize) < levels.len() {
				dones[score.level as usize] = true;
			}
		}
		for done in dones {
			if !done {
				break;
			}
			level += 1;
		}

		let mut g = Game {
			quit: false,
			status: Status::Play,
			must_draw: true,
			debug: false,
			levels,
			snapshots: Vec::new(),
			state: State {
				map: Vec::new(),
				stored: 0,
				px: 0,
				py: 0,
				time_s: 0,
				pushes: 0,
				moves: 0,
				undos: 0,
			},
			undo_states: Vec::new(),
			level,
			last_ticks: SystemTime::now(),
			scores,
			scores_file,
			bw: 0,
			bh: 0,
			width,
			height,
			font,
		};
		g.set_level(level);
		g
	}

	fn set_level(&mut self, level: usize) -> bool {
		if level < self.levels.len() {
			self.status = Status::Play;
			self.must_draw = true;
			self.level = level;
			self.state.moves = 0;
			self.state.pushes = 0;
			self.state.time_s = 0;
			self.state.map = self.levels[level].map.clone();
			self.undo_states = Vec::new();
			self.levels[self.level].crates = self.levels[self.level].crates;
			self.state.stored = self.levels[level].stored;
			self.levels[self.level].w = self.levels[level].w;
			self.levels[self.level].h = self.levels[level].h;
			self.state.px = self.levels[level].px;
			self.state.py = self.levels[level].py;
			self.bw = self.width / self.levels[self.level].w;
			self.bh = (self.height - TEXT_SIZE * TEXT_RATIO) / self.levels[self.level].h;
			true
		} else {
			false
		}
	}

	fn can_move(&self, x: usize, y: usize) -> bool {
		if x < self.levels[self.level].w && y < self.levels[self.level].h {
			let e = self.state.map[y][x];
			if e == EMPTY || e == STORE {
				return true;
			}
		}
		false
	}

	/// Try to move to x+dx:y+dy and also push to x+2dx:y+2dy
	fn try_move(&mut self, dx: isize, dy: isize) {
		let mut do_it = false;
		let x = self.state.px as isize + dx;
		let y = self.state.py as isize + dy;
		if x < 0 || y < 0 {
			return;
		}
		let x = x as usize;
		let y = y as usize;
		if x >= self.levels[self.level].w || y >= self.levels[self.level].h {
			return;
		}
		if self.state.map[y][x] & CRATE == CRATE {
			let to_x = (x as isize + dx) as usize;
			let to_y = (y as isize + dy) as usize;
			if self.can_move(to_x, to_y) {
				do_it = true;
				self.push_undo(true);
				self.state.pushes += 1;
				self.state.map[y][x] &= !CRATE;
				if self.state.map[y][x] & STORE == STORE {
					self.state.stored -= 1;
				}
				self.state.map[to_y][to_x] |= CRATE;
				if self.state.map[to_y][to_x] & STORE == STORE {
					self.state.stored += 1;
					if self.state.stored == self.levels[self.level].crates {
						self.status = Status::Win;
					}
				}
			}
		} else {
			do_it = self.can_move(x, y);
			if do_it {
				self.push_undo(false);
			}
		}
		if do_it {
			self.state.moves += 1;
			self.state.px = x;
			self.state.py = y;
			if let Status::Win = self.status {
				self.save_score();
				self.save_scores();
			}
			self.debug_dump();
			self.must_draw = true;
		}
	}

	fn draw_map(
		&mut self,
		canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
		textures: &[sdl2::render::Texture<'_>],
		texture_creator: &sdl2::render::TextureCreator<sdl2::video::WindowContext>,
	) {
		let curr_ticks = SystemTime::now();
		let duration = curr_ticks
			.duration_since(self.last_ticks)
			.unwrap()
			.as_millis();
		if duration > 1000 {
			if let Status::Play = self.status {
				self.state.time_s += (duration / 1000) as u32;
			}
			self.last_ticks = curr_ticks;
			self.must_draw = true;
		}
		if self.must_draw {
			canvas.set_draw_color(Color::RGB(0, 0, 0));
			canvas.clear();
			// bottom status bar
			canvas
				.copy(
					&textures[N_WHITE],
					None,
					Rect::new(
						0,
						(self.height - TEXT_SIZE * TEXT_RATIO) as i32,
						self.width as u32,
						(TEXT_SIZE * TEXT_RATIO) as u32,
					),
				)
				.expect("Couldn't copy texture into window");
			let x = (WIDTH - self.levels[self.level].w * self.bw) / 2;
			let y = 0;
			for (j, line) in self.state.map.iter().enumerate() {
				for (i, &e) in line.iter().enumerate() {
					let idx = if e == EMPTY {
						if self.state.px == i && self.state.py == j {
							N_PLAYER
						} else {
							N_EMPTY
						}
					} else if e == STORE {
						if self.state.px == i && self.state.py == j {
							N_SPLAYER
						} else {
							N_STORE
						}
					} else if e == CRATE {
						N_CRATE
					} else if e == WALL {
						N_WALL
					} else if e == CRATE | STORE {
						N_STORED
					} else {
						N_EMPTY
					};
					canvas
						.copy(
							&textures[idx as usize],
							None,
							Rect::new(
								(x + i * self.bw) as i32,
								(y + j * self.bh) as i32,
								self.bw as u32,
								self.bh as u32,
							),
						)
						.expect("Couldn't copy texture into window");
				}
			}
			let state = match self.status {
				Status::Win => "You win! Press Return..",
				Status::Pause => "*PAUSE* Press Space..",
				_ => "",
			};
			let ts = self.state.time_s % 60;
			let tm = (self.state.time_s / 60) % 60;
			let th = self.state.time_s / 3600;
			let text = format!(
				"{:02}| moves: {:04} pushes: {:04} time:{}:{:02}:{:02} {}",
				self.level + 1,
				self.state.moves,
				self.state.pushes,
				th,
				tm,
				ts,
				state
			);
			let texture = create_texture_from_text(texture_creator, &self.font, &text, 0, 0, 0)
				.expect("Cannot render text");
			canvas
				.copy(
					&texture,
					None,
					Some(Rect::new(
						0,
						(self.height - TEXT_SIZE * TEXT_RATIO - 4) as i32,
						(text.len() * 5 * ZOOM) as u32,
						(12 * ZOOM) as u32,
					)),
				)
				.expect("Couldn't copy text");
			canvas.present();
			self.must_draw = false;
		}
	}

	fn handle_events(&mut self, event_pump: &mut sdl2::EventPump) {
		for event in event_pump.poll_iter() {
			match event {
				Event::Quit { .. } => {
					self.quit = true;
					break;
				}
				Event::KeyDown {
					keycode: Some(k), ..
				} => match k {
					Keycode::Escape => {
						self.quit = true;
						break;
					}
					Keycode::D => {
						self.debug = !self.debug;
						self.debug_dump();
						continue;
					}
					_ => {}
				},
				_ => {}
			}
			if !match self.status {
				Status::Play => self.handle_event_play(event),
				Status::Pause => self.handle_event_pause(event),
				Status::Win => self.handle_event_win(event),
			} {
				break;
			}
		}
	}

	fn handle_event_play(&mut self, event: sdl2::event::Event) -> bool {
		let mut cont = true;
		if let Event::KeyDown {
			keycode: Some(k), ..
		} = event
		{
			match k {
				Keycode::Space => {
					self.status = Status::Pause;
					self.must_draw = true;
					cont = false;
				}
				Keycode::R => {
					self.set_level(self.level);
					cont = false;
				}
				Keycode::W => {
					self.status = Status::Win;
					self.must_draw = true;
					cont = false;
				}
				Keycode::U => {
					self.pop_undo();
				}
				Keycode::Up => {
					self.try_move(0, -1);
				}
				Keycode::Down => {
					self.try_move(0, 1);
				}
				Keycode::Left => {
					self.try_move(-1, 0);
				}
				Keycode::Right => {
					self.try_move(1, 0);
				}
				_ => {}
			}
		}
		cont
	}

	fn handle_event_pause(&mut self, event: sdl2::event::Event) -> bool {
		let mut cont = true;
		if let Event::KeyDown {
			keycode: Some(k), ..
		} = event
		{
			if let Keycode::Space = k {
				self.status = Status::Play;
				self.must_draw = true;
				cont = false;
			}
		}
		cont
	}

	fn handle_event_win(&mut self, event: sdl2::event::Event) -> bool {
		let mut cont = true;
		if let Event::KeyDown {
			keycode: Some(k), ..
		} = event
		{
			match k {
				Keycode::Return => {
					if self.set_level(self.level + 1) {
					} else {
						println!("Game over.");
						self.quit = true;
						cont = false;
					}
				}
				Keycode::R => {
					self.set_level(self.level);
					cont = false;
				}
				_ => {}
			}
		}
		cont
	}

	fn sleep(&self) {
		sleep(Duration::new(0, 1_000_000_000u32 / 60));
	}
}

fn create_texture_from_text<'a>(
	texture_creator: &'a TextureCreator<WindowContext>,
	font: &sdl2::ttf::Font,
	text: &str,
	r: u8,
	g: u8,
	b: u8,
) -> Option<Texture<'a>> {
	if let Ok(surface) = font.render(text).blended(Color::RGB(r, g, b)) {
		texture_creator.create_texture_from_surface(&surface).ok()
	} else {
		None
	}
}

fn create_texture_rect<'a>(
	canvas: &mut Canvas<Window>,
	texture_creator: &'a TextureCreator<WindowContext>,
	r: u8,
	g: u8,
	b: u8,
	width: u32,
	height: u32,
) -> Option<Texture<'a>> {
	if let Ok(mut square_texture) = texture_creator.create_texture_target(None, width, height) {
		canvas
			.with_texture_canvas(&mut square_texture, |texture| {
				texture.set_draw_color(Color::RGB(r, g, b));
				texture.clear();
			})
			.expect("Failed to color a texture");
		Some(square_texture)
	} else {
		None
	}
}

fn load_texture<'a>(
	root_dir: &std::path::Path,
	texture_creator: &'a TextureCreator<WindowContext>,
	file: &str,
) -> Option<Texture<'a>> {
	let file = root_dir.join("res").join("images").join(file);
	let file = file.to_str().unwrap();
	Some(texture_creator.load_texture(file).unwrap())
}

fn main() {
	let width = WIDTH;
	let height = HEIGHT;
	let sdl_context = sdl2::init().expect("SDL initialization failed");
	let ttf_context = sdl2::ttf::init().expect("SDL TTF initialization failed");
	let _image_context = sdl2::image::init(InitFlag::PNG).unwrap();
	let video_subsystem = sdl_context
		.video()
		.expect("Couldn't get SDL video subsystem");
	let window = video_subsystem
		.window(TITLE, width as u32, height as u32)
		.position_centered()
		.build()
		.expect("Failed to create window");
	let mut canvas = window
		.into_canvas()
		.target_texture()
		.present_vsync()
		.build()
		.expect("Couldn't get window's canvas");
	let mut event_pump = sdl_context.event_pump().expect(
		"Failed to get
          SDL event pump",
	);
	let texture_creator: TextureCreator<_> = canvas.texture_creator();
	let root_dir = current_exe().unwrap();
	let root_dir = root_dir
		.parent()
		.unwrap()
		.parent()
		.unwrap()
		.parent()
		.unwrap();
	let mut game = Game::new(&ttf_context, root_dir, width, height);
	macro_rules! texture {
		($r:expr, $g:expr, $b:expr) => {
			create_texture_rect(
				&mut canvas,
				&texture_creator,
				$r,
				$g,
				$b,
				game.bw as u32,
				game.bh as u32,
				)
			.unwrap()
		};
		($file:expr) => {
			load_texture(root_dir, &texture_creator, $file).unwrap()
		};
	}
	let textures = [
		texture!(I_EMPTY),
		texture!(I_STORE),
		texture!(I_STORED),
		texture!(I_CRATE),
		texture!(I_PLAYER),
		texture!(I_SPLAYER),
		texture!(I_WALL),
		texture!(255, 255, 255),
	];
	while !game.quit {
		game.handle_events(&mut event_pump);
		game.draw_map(&mut canvas, &textures, &texture_creator);
		game.sleep();
	}
}
