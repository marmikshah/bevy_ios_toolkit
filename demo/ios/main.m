// Native iOS entry point for the bevy_ios_toolkit demo. Delegates to the Rust
// `main_rs` symbol exported by the bevy_ios_toolkit_demo static library; winit's
// iOS backend then drives the UIApplication lifecycle from inside the Bevy app.
//
// The toolkit's Swift shims (StoreKitBridge / AdMobBridge / AppTrackingBridge /
// GameKitBridge / ReviewBridge / PlatformBridge, listed in project.yml) expose
// @_cdecl symbols the Rust side calls; they need no entry point of their own.
#import <UIKit/UIKit.h>

extern void main_rs(void);

int main(int argc, char *argv[]) {
    main_rs();
    return 0;
}
