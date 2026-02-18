use wesl::Wesl;

fn main() {
    let mut wesl = Wesl::new("shaders");
    wesl.use_sourcemap(true);
    wesl.set_options(wesl::CompileOptions {
        imports: true,
        condcomp: false,
        generics: false,
        strip: true,
        lower: true,
        validate: true,
        ..Default::default()
    });

    wesl.build_artifact(&"package::clear".parse().unwrap(), "clear");
    wesl.build_artifact(&"package::xfb_blit".parse().unwrap(), "xfb_blit");
    wesl.build_artifact(&"package::color_blit".parse().unwrap(), "color_blit");
    wesl.build_artifact(&"package::depth_blit".parse().unwrap(), "depth_blit");
    wesl.build_artifact(&"package::color_convert".parse().unwrap(), "color_convert");
    wesl.build_artifact(&"package::depth_convert".parse().unwrap(), "depth_convert");
    wesl.build_artifact(&"package::depth_resolve".parse().unwrap(), "depth_resolve");
}
