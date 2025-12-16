use std::collections::BTreeMap;
use std::io::Write;
use pipa::ir::{gen_ir, dump_ir};
use pipa::syntax::ast;
use pipa::vm::Vm;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct App {
    scale: f32,
    new_var: (String, String),
    new_array: (String, String),
    vars: BTreeMap<String, String>,
    arrays: BTreeMap<String, String>,
    code: String,
    console: String,
    output: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            scale: 1.0,
            new_var: (String::new(), String::new()),
            new_array: (String::new(), String::new()),
            vars: BTreeMap::from([
                ("name".into(), "jon".into()),
                ("sirname".into(), "doe".into())
            ]),
            arrays: BTreeMap::from([
                ("LIST".into(), "first\nsecond\nthird".into()),
            ]),
            console: String::new(),
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
            output: String::new(),
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
                ui.add(editor);
                // execution
                if ui.button("Run").clicked() {
                    run_vm(self);
                }
                // console 
                ui.collapsing("Console", |ui| {
                    ui.code(&self.console);
                });
                ui.separator();
                ui.label("Output:");
                ui.code(&self.output);
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

fn run_vm(state: &mut App) {
    state.code = state.code.replace("\t", "    ");
    let mut output = Vec::new();
    // tokenize + lex
    let tokens = match ast(&state.code) {
        Ok(r) => r, 
        Err(e) => { 
            e.write_message(&mut output, "index.pipa", &state.code).unwrap();
            state.output = String::from_utf8(output).unwrap();
            return;
        }
    };

    // ir
    let ir = match gen_ir(&state.code, tokens) {
        Ok(ir) => ir,
        Err(e) => { 
            e.write_message(&mut output, "index.pipa", &state.code).unwrap();
            state.output = String::from_utf8(output).unwrap();
            return;
        }
    };
    // convert vars
    let mut vars = BTreeMap::new();
    let mut arrays = BTreeMap::new();

    for (key, value) in state.vars.clone() {
        vars.insert(key.into(), value.into());
    }

    for (key, value) in state.arrays.clone() {
        arrays.insert(key.into(), value.lines().map(|s| s.into()).collect());
    }

    // run
    let mut vm = Vm::new(vars, arrays);

    match vm.run(&mut output, &ir) {
        Ok(_) => {
        },
        Err(e) => {
            dbg!(e);
        }
    }
    
    // fill console
    let mut console = Vec::new();
    vm.dump_state(&mut console).unwrap();
    write!(&mut console, "\n").unwrap();
    dump_ir(&mut console, &ir).unwrap();

    // save changes
    state.output = String::from_utf8(output).unwrap();
    state.console = String::from_utf8(console).unwrap();
}
