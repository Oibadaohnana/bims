// Dumps one frame of the real draw buffer as SVG, so the room can be looked at
// without a browser. Handy after a layout change: a bed half inside the heads
// is obvious here and invisible in every assertion.

include!("modules.rs");

use game::Game;

fn main() {
    let mut game = Game::new(21, 960.0, 640.0);
    // Sweeping needs something to sweep.
    if std::env::args().nth(1).as_deref() == Some("sweep") {
        for y in (100..500).step_by(52) {
            for x in (100..760).step_by(52) {
                let at = math::vec2(x as f32, y as f32);
                if game.spot_at(at.x, at.y) == room::SPOT_DECK && game.can_reach_for_probe(0, at) {
                    game.foul_for_probe(at);
                }
            }
        }
    }
    // A graded row of mess, for looking at what each depth actually reads as
    // on the deck. Left to right: one dirty job's stain, a smear walked out of
    // a fouled tile, the smear a step further on, and the worst there is. The
    // scale is linear and the shading is not, so this is the only way to tell
    // whether a stain is visible at all.
    if std::env::args().nth(1).as_deref() == Some("grime") {
        for (i, cost) in [14.0, 27.5, 6.9, 110.0].iter().enumerate() {
            game.soil_for_probe(math::vec2(240.0 + i as f32 * 52.0, 300.0), *cost);
        }
    }
    // Run to whatever the first argument asks for: "start" for a few frames
    // in, "bed" for the first moment somebody is asleep, "table" for the first
    // moment somebody is at a meal.
    let want = std::env::args().nth(1).unwrap_or_else(|| "start".into());
    // Sitting on the deck for want of anybody to talk to. Pinned forward and
    // re-pinned each frame, because the other one coming over for a word
    // would put the clock straight back to nothing.
    let sulking = want == "sad";
    let target: u32 = match want.as_str() {
        "bed" => 4,
        "table" => 1,
        "sweep" => 13,
        "talk" => 14,
        _ => 0,
    };
    let mut n = 0;
    loop {
        if sulking {
            game.leave_alone_for_probe(bim::PLAYER, 6.0);
        }
        game.update(1.0 / 60.0);
        n += 1;
        if sulking {
            if game.broken_down_for_probe(bim::PLAYER) {
                break;
            }
        } else if target == 0 && n >= 120 {
            break;
        } else if target == 14 {
            // The errand starting is the walk to the meeting point starting,
            // and a picture of two Bims converging on the middle of the deck
            // is not a picture of a conversation. `chat_topic` is non-zero
            // only once somebody is actually standing there talking.
            if (0..bim::CREW).any(|w| game.chat_topic(w) != 0) {
                break;
            }
        } else if target != 0 && (0..bim::CREW).any(|w| game.activity(w) == target) {
            break;
        }
        if n > 24 * 60 * 60 * 3 {
            eprintln!("never reached it");
            break;
        }
    }
    // Reaching the errand is only the walk starting. A few game minutes more
    // and the Bim is actually in the bed or on the chair, which is the thing
    // worth looking at. A conversation is caught mid-way for the same reason:
    // the walk to the meeting spot is not the thing to look at.
    if target != 0 {
        let settle = if target == 14 { 0 } else { 3 * 60 * 60 };
        for _ in 0..settle {
            game.update(1.0 / 60.0);
        }
    }
    eprintln!("frame {n}");
    let shapes: Vec<f32> = {
        let ptr = game.draw_ptr();
        let len = game.draw_len();
        unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
    };

    let stride = draw::STRIDE;
    println!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\">",
        room::ROOM_W,
        room::ROOM_H,
        room::ROOM_W,
        room::ROOM_H
    );
    println!("<rect width=\"100%\" height=\"100%\" fill=\"#0c1210\"/>");
    for s in shapes.chunks(stride) {
        let (kind, x, y, w, h, rot, radius, line) =
            (s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]);
        let (r, g, b, a) = (
            (s[8] * 255.0).round(),
            (s[9] * 255.0).round(),
            (s[10] * 255.0).round(),
            s[11],
        );
        let paint = if line > 0.0 {
            format!("fill=\"none\" stroke=\"rgb({r},{g},{b})\" stroke-opacity=\"{a}\" stroke-width=\"{line}\"")
        } else {
            format!("fill=\"rgb({r},{g},{b})\" fill-opacity=\"{a}\"")
        };
        let turn = rot.to_degrees();
        if kind == 1.0 {
            println!(
                "<ellipse cx=\"0\" cy=\"0\" rx=\"{}\" ry=\"{}\" {paint} transform=\"translate({x} {y}) rotate({turn})\"/>",
                w.abs() / 2.0,
                h.abs() / 2.0
            );
        } else {
            println!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{radius}\" {paint} transform=\"translate({x} {y}) rotate({turn})\"/>",
                -w / 2.0,
                -h / 2.0,
                w.abs(),
                h.abs()
            );
        }
    }
    // The names, which the host draws rather than the buffer.
    for (who, name) in ["James", "Kate"].iter().enumerate() {
        let p = game.bim_pos(who);
        println!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"13\" font-weight=\"600\" fill=\"{}\" stroke=\"rgba(6,10,9,0.85)\" stroke-width=\"3\" paint-order=\"stroke\">{name}</text>",
            p.x,
            p.y - 46.0,
            if who == 0 { "#7fd1a8" } else { "#e7efe9" }
        );
    }
    println!("</svg>");
}
