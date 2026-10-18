// Native iOS entry point for the bevy_ios_toolkit demo. Delegates to the Rust
// `main_rs` symbol exported by the bevy_ios_toolkit_demo static library; winit's
// iOS backend then drives the UIApplication lifecycle from inside the Bevy app.
// The toolkit's Swift shims come from its SPM products and need no entry point.
#import <UIKit/UIKit.h>

extern void main_rs(void);

int main(int argc, char *argv[]) {
    main_rs();
    return 0;
}
