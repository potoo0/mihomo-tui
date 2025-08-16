use color_eyre::Result;
use tracing::error;

pub fn init() -> Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().try_into_hooks()?;
    eyre_hook.install()?;
    std::panic::set_hook(Box::new(move |panic_info| {
        if let Ok(mut t) = crate::tui::Tui::new()
            && let Err(r) = t.exit()
        {
            error!("Unable to exit Terminal: {:?}", r);
        }

        eprintln!("{}", panic_hook.panic_report(panic_info));
    }));
    Ok(())
}
