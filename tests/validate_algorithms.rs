use corgi_lib::image_gen::hpf_algorithm;
use corgi_lib::shared::algorithms::perturbed_32;
use corgi_lib::shared::types::{BufferValues, ComputeParams};
use corgi_lib::shared::wgsl_primitives::{Vec2, VecN};
use corgi_lib::types::serde::SafeSaveLoad;
use corgi_lib::types::{Algorithm, ComplexPoint, ImgSpec, Style, get_precision};
use rug::Float;
use rug::ops::Pow;

use crate::common::test_output_dir;

mod common;

macro_rules! algorithm_test {
    ($(#[$ignore:meta])? $name:ident, ($x:literal, $y:literal),  ($px:literal, $py:literal), $zoom:literal) => {
        $(#[$ignore])?
        #[test]
        fn $name() {
            perturbed_matches_hpf(
                stringify!($name),
                corgi_lib::types::ComplexPoint {
                    x: Float::with_val($x.len() as u32 * 4, Float::parse($x).unwrap()),
                    y: Float::with_val($y.len() as u32 * 4, Float::parse($y).unwrap()),
                },
                Some(corgi_lib::types::ComplexPoint {
                    x: Float::with_val($px.len() as u32 * 4, Float::parse($px).unwrap()),
                    y: Float::with_val($py.len() as u32 * 4, Float::parse($py).unwrap()),
                }),
                $zoom as f32
            )
        }
    };

    ($name:ident, ($x:literal, $y:literal), [$($(#[$ignore:meta])? $zoom:literal),+]) => {
        algorithm_test!(
            _,
            $name,
            corgi_lib::types::ComplexPoint {
                x: Float::with_val($x.len() as u32 * 4, Float::parse($x).unwrap()),
                y: Float::with_val($y.len() as u32 * 4, Float::parse($y).unwrap()),
            },
            [$($(#[$ignore])? $zoom),+]
        );
    };

    (_, $name:ident, $pt:expr, [$($(#[$ignore:meta])? $zoom:literal),+]) => {
        mod $name {
            use super::perturbed_matches_hpf;
            use rug::Float;
            use pastey::paste;
            paste!{
                $(
                    $(#[$ignore])?
                    #[test]
                    fn [< zoom_ $zoom >]() {
                        perturbed_matches_hpf(
                            stringify!([< zoom_ $zoom >]),
                            $pt,
                            None,
                            $zoom as f32
                        )
                    }
                )+
            }
        }
    };
}

algorithm_test!(
    deep_point,
    (
        "-1.74999841099374081749002483162428393452822172335808534616943930976364725846655540417646727085571962736578151132907961927190726789896685696750162524460775546580822744596887978637416593715319388030232414667046419863755743802804780843375",
        "-0.00000000000000165712469295418692325810961981279189026504290127375760405334498110850956047368308707050735960323397389547038231194872482690340369921750514146922400928554011996123112902000856666847088788158433995358406779259404221904755"
    ),
    [
        0,
        #[ignore = "slow"]
        10,
        25,
        #[ignore = "slow"]
        50
    ]
);

algorithm_test!(
    glitch_1,
    (
        "-1.24423450233498219683295648874641923793324000174806008363219410362921036237879036908579",
        "-8.82227488165152191976757899571468615032467682588580505011062108263964148363195614295665e-2"
    ),
    [
        0,
        10,
        20,
        30,
        #[ignore = "the test point for this lands on a noisy spot"]
        50,
        75
    ]
);

algorithm_test!(
    glitch_2,
    (
        "-1.03007338733367357891068755102156668308523121399115053651726212374894561810164140",
        "-3.25444784385027507232770872761473643470804245462922183987775048943842324020128968e-1"
    ),
    [0, 10, 25, 50, 100, 200]
);

algorithm_test!(
    glitch_2_1,
    (
        "-1.03007338733367357891068755102156668308523121399115053651726524887400338355129740",
        "-3.25444784385027507232770872761473643470804245462922183987775031653021835846476888e-1"
    ),
    (
        "-1.0300733873336735789106875510215666830852312139911505365172551465137403167",
        "-3.2544478438502750723277087276147364347080424546292218398780503954889755500e-1"
    ),
    200.0
);

algorithm_test!(
    glitch_2_2,
    (
        "-1.0300733873336735789106875510215666830852312139911505365172621322247011386724654",
        "-3.2544478438502750723277087276147364347080424546292218398777501444286928812953937e-1"
    ),
    (
        "-1.0300733873336735789106875510215666830852312139911505365172621309283654234736184",
        "-3.2544478438502750723277087276147364347080424546292218398777501545231781168562619e-1"
    ),
    200.0
);

algorithm_test!(
    glitch_2_3,
    (
        "-1.03007338733367357891068755102156668308523121399115053651726213874462848895400444",
        "-3.25444784385027507232770872761473643470804245462922183987775008606379781445914e-1"
    ),
    (
        "-1.03007338733367357891068755102156668308523121399115053651726209691640661000392",
        "-3.25444784385027507232770872761473643470804245462922183987775038237959677939949e-1"
    ),
    200.0
);

algorithm_test!(
    #[ignore = "slow"]
    glitch_2_4,
    (
        "-1.2442345023349821968329564887464192379332400017480600836321941036292055996801055299238339",
        "-8.8222748816515219197675789957146861503246768258858050501106210826391807500143117999682405e-2"
    ),
    (
        "-1.24423450233498219683295648874641923793324000174806008363219410362920458856301926132698",
        "-8.82227488165152191976757899571468615032467682588580505011062108263926143161298338796456e-2"
    ),
    230.0
);

algorithm_test!(
    #[ignore = "slow"]
    deep_glitch,
    (
        "-1.2442345023349821968329564887464192379332400017480600836321941036292056398920203082286454",
        "-8.8222748816515219197675789957146861503246768258858050501106210826392945625979888391513518e-2"
    ),
    (
        "-1.244234502334982196832956488746419237933240001748060083632194103629205456651888849159467",
        "-8.822274881651521919767578995714686150324676825885805050110621082639297351551951453783932e-2"
    ),
    232.378
);

fn perturbed_matches_hpf(name: &str, cpt: ComplexPoint, probe_pt: Option<ComplexPoint>, zoom: f32) {
    let prec = get_precision(zoom);
    let image = ImgSpec {
        location: corgi_lib::types::Location {
            fractal_kind: corgi_lib::types::FractalKind::Mandelbrot,
            center: cpt.clone(),
            zoom,
            angle: 0.0,
            max_iter: 100_000,
            probe_location: probe_pt.unwrap_or(corgi_lib::types::ComplexPoint {
                x: cpt.x.clone() + Float::with_val(prec, 2.0).pow(-zoom - 0.0),
                y: cpt.y.clone() + Float::with_val(prec, 2.0).pow(-zoom - 0.0),
            }),
        },
        style: Style::opt_default(),
        width: 2,
        height: 2,
        samples: 1,
        optimization_level: corgi_lib::types::OptLevel::AccuracyOptimized,
    };
    let iter_batch_size = 1;
    let probed_data = corgi_lib::image_gen::probe::probe::<f32>(
        &image.location.probe_location,
        image.location.max_iter,
        image.location.zoom,
        None,
        &mut |_| {},
    );
    let parameters =
        ComputeParams::create_with_alg(&image, probed_data.len(), Algorithm::Perturbedf32);
    for index in 0..3 {
        let mut test_bv = BufferValues::zero();
        let mut test_escape = 0;
        let mut hpf_bv = hpf_algorithm::BufferValues::zero();
        let mut hpf_escape = 0;
        for iteration in 0..=(image.location.max_iter / iter_batch_size) {
            // Test error amount
            let test_z = Vec2::from(probed_data[test_bv.ref_iteration as usize])
                + test_bv.delta_n * 2.0.powf(test_bv.zoom);
            let ref_z = hpf_bv.delta_n.to_f32();
            let probe_z = Vec2::from(probed_data[test_bv.ref_iteration as usize]);
            let error = ((ref_z - test_z) / test_z).length();
            println!(
                "== I: {iteration}\nRef Z: {ref_z:?} - Err: {error}\nPer z: {test_z:?} - Delta: {:?}\nProbe: {probe_z:?} - Diff: {:?}",
                test_bv.delta_n * 2.0.powf(test_bv.zoom),
                ref_z - probe_z,
            );
            if error > 1e-1 {
                println!("=== === Error found === ===");
                println!("Error: {error}, Iteration: {iteration}");
                dbg!(&test_bv);
                dbg!(ref_z - test_z);
                dbg!(ref_z - probe_z);
                println!("=== === Error found === ===");
            }

            // Test for escape
            if test_bv.step != 0 && test_escape == 0 {
                test_escape = iteration;
            }
            if hpf_bv.step != 0 && hpf_escape == 0 {
                hpf_escape = iteration;
            }
            if test_escape != 0 && hpf_escape != 0 {
                if (hpf_escape as f64 - test_escape as f64) / iteration as f64 > 0.01 {
                    println!("Test Points did not match!");
                    image
                        .save(&test_output_dir().join(format!("{name}_failure.corg")))
                        .unwrap();
                    dbg!(test_escape, hpf_escape);
                    panic!()
                }
                break;
            }

            // Run next step
            let parameters = parameters.with_iter(&image, iteration, iter_batch_size);
            if parameters.chunk_max_iter == 0 {
                break;
            }
            test_bv = perturbed_32::calculate_point(
                Vec2::new(index % 2, index / 2),
                test_bv.clone(),
                image.get_flags(),
                parameters,
                bytemuck::cast_slice(&probed_data),
            );
            hpf_bv = hpf_algorithm::calculate_point(
                Vec2::new(index % 2, index / 2),
                hpf_bv.clone(),
                image.get_flags(),
                parameters,
                cpt.clone(),
            );
        }
    }
}
