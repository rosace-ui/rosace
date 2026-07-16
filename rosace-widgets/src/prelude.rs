//! Convenience re-exports for all built-in widgets.
//!
//! Add `use rosace_widgets::prelude::*;` to bring every widget into scope.

pub use crate::{
    Widget, Alignment, PaintCtx, BoxedWidget, WidgetApp,
    TextAlign, FontWeight,
    AppBar, Avatar, Badge, BottomNavItem, BottomNavigationBar, Button, ButtonVariant, FloatingActionButton, SearchBar, Snackbar,
    Carousel, PageView, RatingBar, Stepper, Table, TableColumn,
    Card, Checkbox, Chip, Column, Container, CustomPaint,
    AspectRatio, BoxShape, CircularProgress, Dropdown, Drawer, Expander, Dialog, Divider, EdgeInsets, Grid, Expanded, Hero, HeroApi, Icon, IconKind,
    ListTile, ListView, Menu, NavItem, NavRail, ProgressBar,
    AbsorbPointer, IgnorePointer, PressApi, Pressable, RectReader, RepaintBoundary, TransformLayer,
    Positioned, Radio, Row, Scaffold, ScreenTransitionView, SegmentedControl, ScrollView, ScrollAxis, Sheet, Slider, Spacer, Stack, Switch,
    Skeleton, Tab, TabBar, Wrap, Text, TextArea, TextInput, Toast, ToastKind, Tooltip,
    CursorShape, CursorStyle, EditController, InputFilter, Span,
    Image, ImageCache, ImageFit, ImageSource, ImageWidget,
};
pub use rosace_forms::{Form, FormField, FieldError, Validator, Required, Email, MinLength, MaxLength, Range, Contains};
