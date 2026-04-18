use std::collections::BTreeMap;
use std::time::Duration;
use web_time::SystemTime;
use pipa::ir::{gen_ir, dump_ir, Op};
use pipa::analysis::{NO_OPT, FULL_OPT};
use pipa::syntax::{ast, Node};
use pipa::vm::Vm;


const AUTO_RUN_DURATION: Duration = Duration::new(1, 0);

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct App {
    scale: f32,
    optimize: bool,
    has_err: bool,
    #[serde(skip)]
    last_edit: Option<SystemTime>,
    new_var: (String, String),
    new_array: (String, String),
    vars: BTreeMap<String, String>,
    arrays: BTreeMap<String, String>,
    code: String,
    #[serde(skip)]
    ast: Vec<Node>,
    #[serde(skip)]
    ir: Vec<Op>,
    ir_view: Vec<u8>,
    output: Vec<u8>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            scale: 1.0,
            optimize: true,
            has_err: false,
            last_edit: None,
            new_var: (String::new(), String::new()),
            new_array: (String::new(), String::new()),
            vars: BTreeMap::from([
                ("name".into(), "jon".into()),
                ("sirname".into(), "doe".into())
            ]),
            arrays: BTreeMap::from([
                ("LIST".into(), "first\nsecond\nthird".into()),
            ]),
            code: String::from(
r#"<!DOCTYPE html>
<html>
  <head>
    <title>This is a hello page</title>
  </head>
  <body>
    <div>
        <p>
        {{
            # stirng formatting
            "\"Hello, $(name) $(sirname)\""
        }}
        <p>This page is generated using the pipa language</p>
        <ul>
          {{
            # macro
            @print_item "$(_index_): $(_item_)" | "\n\t\t\t<li>$(_)</li>"

            # arrays
            LIST[:] | ?print_item
          }}
        </ul>
    </div>
  </body>
</html>"#),
            ast: Vec::new(),
            ir: Vec::new(),
            ir_view: Vec::new(),
            output: Vec::new(),
        }
    }
}

impl App {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}

impl eframe::App for App {
    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui
        ctx.set_theme(egui::Theme::Light);
        ctx.set_pixels_per_point(self.scale);


        egui::CentralPanel::default().show(&ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("pipa playground");
                ui.separator();
                // scale
                ui.horizontal(|ui| {
                    ui.label("Page scale:");
                    if ui.button("-").clicked() {
                        let v = self.scale - 0.5;
                        self.scale = if v < 1.0 { 1.0 } else { v }
                    }
                    ui.label(self.scale.to_string());
                    if ui.button("+").clicked() {
                        let v = self.scale + 0.5;
                        self.scale = if v > 5.0 { 5.0 } else { v }
                    }
                    ui.hyperlink_to("Examples", "https://github.com/GachiLord/pipa/tree/main/examples")
                });
                // display vars
                ui.label("Constants:");
                vars_editor(self, ui);
                // arrays
                ui.separator();
                ui.label("Arrays(separated by a newline):");
                arrays_editor(self, ui);
                ui.separator();
                // editor
                let editor = egui::TextEdit::multiline(&mut self.code)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(10);
                let editor_response = ui.add(editor);
                // optimizations
                if ui.checkbox(&mut self.optimize, "Optimize").changed() {
                    compile(self);
                }
                // handle first run
                if self.ir_view.is_empty() || self.ast.is_empty() {
                    compile(self);
                }
                // compilation
                if editor_response.changed() {
                    self.last_edit = Some(SystemTime::now());
                }
                if editor_response.lost_focus() {
                    compile(self);
                }
                if let Some(edit) = self.last_edit {

                    if edit.elapsed().unwrap() >= AUTO_RUN_DURATION {
                        compile(self);
                        run_vm(self);
                        self.last_edit = None;
                    }
                }
                // execution
                if ui.button("Run").clicked() {
                    run_vm(self);
                }
                // ir
                ui.collapsing("IR", |ui| {
                    ui.code(str::from_utf8(&self.ir_view).unwrap());
                });
                // ast
                ui.collapsing("AST", |ui| {
                    ast_view(&self.code, &self.ast, ui, 0);
                });
                // output
                ui.separator();
                ui.label("Output:");
                ui.code(str::from_utf8(&self.output).unwrap());
            });
        });
    }
}

fn vars_editor(state: &mut App, ui: &mut egui::Ui) {
    let mut to_delete = Vec::with_capacity(state.vars.len());
    for (key, value) in state.vars.iter_mut() {
        ui.horizontal(|ui| {
            ui.label(key);
            ui.add(egui::TextEdit::multiline(value).desired_rows(1));
            if ui.button("Remove").clicked() {
                to_delete.push(key.to_owned());
            }
        });
    }
    for var in to_delete {
        state.vars.remove(&var);
    }
    // add vars
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.new_var.0).hint_text("Name"));
        ui.add(egui::TextEdit::multiline(&mut state.new_var.1).desired_rows(1).hint_text("Value"));
        if ui.button("Add").clicked() {
            let key: String = state.new_var.0.drain(..).collect();
            state.vars.insert(key, state.new_var.1.drain(..).collect());
        }
    });
}

fn arrays_editor(state: &mut App, ui: &mut egui::Ui) {
    let mut to_delete = Vec::with_capacity(state.arrays.len());
    for (key, value) in state.arrays.iter_mut() {
        ui.horizontal(|ui| {
            ui.label(key);
            ui.add(egui::TextEdit::multiline(value).desired_rows(1));
            if ui.button("Remove").clicked() {
                to_delete.push(key.to_owned());
            }
        });
    }
    for var in to_delete {
        state.arrays.remove(&var);
    }
    // add vars
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.new_array.0).hint_text("Name"));
        ui.add(egui::TextEdit::multiline(&mut state.new_array.1).desired_rows(1).hint_text("Values"));
        if ui.button("Add").clicked() {
            state.arrays.insert(state.new_array.0.drain(..).collect(), state.new_array.1.drain(..).collect());
        }
    });
}

fn ast_view(code: &str, ast: &Vec<Node>, ui: &mut egui::Ui, mut parent_id: usize) {
    for node in ast {
        let s = code.get(node.first_char..node.end_char)
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or_default();

        parent_id += 1;

        if node.children.is_empty() {
            ui.push_id(parent_id, |ui| ui.label(s));
        } else {
            ui.push_id(parent_id, |ui| {
                ui.collapsing(s, |ui| {
                    ast_view(code, &node.children, ui, parent_id + 1);
                });
            });
        }
    }
}

fn compile(state: &mut App) {
    // tokenize + lex
    match ast(&state.code) {
        Ok(r) => {
            state.ast = r;
            state.has_err = false;
        }, 
        Err(e) => { 
            state.output.clear();
            e.write_message(&mut state.output, "index.pipa", &state.code).unwrap();
            state.has_err = true;
            return;
        }
    };
    let opt = if state.optimize { FULL_OPT } else { NO_OPT };

    // ir
    match gen_ir(&state.code, state.ast.clone(), opt) {
        Ok(ir) => {
            state.ir = ir;
            state.has_err = false;
        },
        Err(e) => {
            state.output.clear();
            e.write_message(&mut state.output, "index.pipa", &state.code).unwrap();
            state.has_err = true;
            return;
        }
    };

    state.ir_view.clear();
    dump_ir(&mut state.ir_view, &state.ir).unwrap();
}

fn run_vm(state: &mut App) {
    // convert arrays
    let mut arrays = BTreeMap::new();

    for (key, value) in state.arrays.clone() {
        arrays.insert(key.into(), value.lines().map(|s| s.into()).collect());
    }

    // compile if first run
    if state.has_err {
        return;
    }
    // clean output before running
    state.output.clear();
    // run
    let mut vm = Vm::new(&state.vars, &arrays);

    match vm.run(&mut state.output, &state.ir) {
        Ok(_) => {
        },
        Err(e) => {
            dbg!(e);
        }
    }
}
