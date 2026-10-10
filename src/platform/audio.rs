//! Platform selection for the sound backend.
//! WASM gets the WebAudio graph; native plays through rodio.

use crate::matrix_game::sound::SoundOutput;

pub fn make_output() -> Box<dyn SoundOutput> {
    #[cfg(target_arch = "wasm32")]
    {
        Box::new(super::audio_web::WebOutput::new())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(super::audio_native::NativeOutput::new())
    }
}
