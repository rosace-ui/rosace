//! `rosace-widgets` — built-in widgets for the ROSACE UI framework.

/// Composable widget tree system — the primary API for building ROSACE apps.
pub mod tree;

pub mod image;
pub mod prelude;

/// Template descriptor (D103 / D102 Tier 1) — the data form of a `view!` tree
/// for universal hot reload; see the module docs.
pub mod template;
pub use template::{PropValue, StaticValue, Template, TemplateKey, TemplateNode};

// ── Tree widget re-exports (canonical top-level names) ─────────────────────
pub use tree::{Alignment, Children, Semantics, Widget};
pub use tree::PaintCtx;
pub use tree::{HitTarget, ScrollTarget};
pub use tree::{AbsorbPointer, IgnorePointer};
pub use tree::{LongPressable, PressApi, Pressable};
pub use tree::BoxedWidget;
pub use tree::WidgetApp;
pub use tree::AppBar;
pub use tree::{BottomNavItem, BottomNavigationBar, FloatingActionButton, SearchBar, Snackbar};
pub use tree::{Carousel, PageView, RatingBar, Stepper, Table, TableColumn};
pub use tree::{InteractiveViewer, DatePicker, SimpleDate, TimePicker, SimpleTime, DataTable, DataTableColumn, SortDirection};
pub use tree::{
    ShaderPaint, MaterialKey, resolve_material, ContainerMaterial, CardMaterial,
    DialogMaterial, SheetMaterial, DrawerMaterial, AppBarMaterial, BottomNavMaterial,
};
pub use tree::{GlassLens, SelectionKind, SelectionStyle};
pub use tree::Avatar;
pub use tree::Badge;
pub use tree::Button;
pub use tree::ButtonVariant;
pub use tree::Card;
pub use tree::Checkbox;
pub use tree::Chip;
pub use tree::Column;
pub use tree::{BoxShape, Container};
pub use tree::{AspectRatio, CircularProgress, Grid, Positioned, Skeleton, Wrap};
pub use tree::{Dropdown, Drawer, Expander, Radio, SegmentedControl};
pub use tree::CustomPaint;
pub use tree::{Dialog, DialogPresentation};
pub use tree::Divider;
pub use tree::EdgeInsets;
pub use tree::Expanded;
pub use tree::{Hero, HeroApi};
pub use tree::Icon;
pub use tree::IconKind;
pub use tree::{register_icon, resolve_icon};
pub use tree::ListTile;
pub use tree::ListView;
pub use tree::Menu;
pub use tree::NavItem;
pub use tree::NavRail;
pub use tree::ProgressBar;
pub use tree::RectReader;
pub use tree::RepaintBoundary;
pub use tree::TransformLayer;
pub use tree::{
    OverlayEntry, LayerId, LayerPosition,
    InputBehavior, FocusBehavior, ScrimConfig,
    push_overlay, drain_overlays, clear_overlays,
};
pub use tree::{OverlayApi, OverlayKind};
pub use tree::FocusApi;
pub use tree::Row;
pub use tree::Scaffold;
pub use tree::ScreenTransitionView;
pub use tree::{ScrollView, ScrollAxis};
pub use tree::Sheet;
pub use tree::{Toast, ToastKind};
pub use tree::Slider;
pub use tree::Spacer;
pub use tree::Stack;
pub use tree::Switch;
pub use tree::Tab;
pub use tree::TabBar;
pub use tree::{Text, TextAlign, FontWeight};
pub use tree::TextInput;
pub use tree::TextArea;
pub use tree::{CursorShape, CursorStyle, EditController, InputFilter, Span, SpanFn, TextLayoutSnapshot};
pub use rosace_forms::{Form, FormField, FieldError, Validator, Required, Email, MinLength, MaxLength, Range, Contains};
pub use tree::Image;
pub use tree::{Tooltip, TooltipStyle, WidgetExt};

pub use image::{DecodedImage, ImageCache, ImageFit, ImageSource, ImageWidget};
