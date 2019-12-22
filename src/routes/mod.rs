//! All routes for the API.
//!
//! Check the root of the crate for the full API layout.

pub mod event_routes;
pub mod from_url;
pub mod member_routes;
pub mod misc_routes;
pub mod officer_routes;
pub mod repertoire_routes;
pub mod router;

use self::event_routes::*;
use self::member_routes::*;
use self::misc_routes::*;
use self::officer_routes::*;
use self::repertoire_routes::*;
use crate::error::{GreaseError, GreaseResult};
use crate::router;
use http::{
    header::{CONTENT_LENGTH, CONTENT_TYPE},
    response,
};
use serde_json::{json, Value};
use std::panic::{self, AssertUnwindSafe};
use url::percent_encoding::{utf8_percent_encode, DEFAULT_ENCODE_SET};

/// The main entry-point for the whole crate.
///
/// Using the [cgi](cgi) crate, requests come in to `stdin` as a stream of
/// bytes, and all headers are set using environment variables. The [cgi](cgi)
/// crate handles putting this into a [Request](http::request::Request) from
/// the [http](http) crate for us.
///
/// This method returns all data as "application/json" responses. On success,
/// a 200 status code is returned, while all errors return appropriate error
/// status codes (See [GreaseError](crate::error::GreaseError) for how those
/// get mapped).
///
/// In the rare case that a `panic!` occurs, this function will attempt
/// to catch it, log it with [log_panic](crate::util::log_panic), and then
/// return a JSON object with some debug information.
pub fn handle_request(mut request: cgi::Request) -> cgi::Response {
    let mut response = None;

    let result = {
        panic::catch_unwind(AssertUnwindSafe(|| {
            if request.method() == "OPTIONS" {
                response = Some(
                    response::Builder::new()
                        .status(200)
                        .header("Allow", "GET, POST, DELETE, OPTIONS")
                        .header("Access-Control-Allow-Origin", "*")
                        .header("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")
                        .header(
                            "Access-Control-Allow-Headers",
                            "token,access-control-allow-origin,content-type",
                        )
                        .body("OK".to_owned().into_bytes())
                        .unwrap(),
                );
                return;
            }

            let uri = {
                let path = request
                    .headers()
                    .get("x-cgi-path-info")
                    .map(|uri| uri.to_str().unwrap())
                    .unwrap_or("/");
                let param_str = request
                    .headers()
                    .get("x-cgi-query-string")
                    .map(|uri| uri.to_str().unwrap())
                    .unwrap_or("");

                format!(
                    "https://gleeclub.gatech.edu{}?{}",
                    utf8_percent_encode(&path, DEFAULT_ENCODE_SET).to_string(),
                    utf8_percent_encode(&param_str, DEFAULT_ENCODE_SET).to_string()
                )
            };

            *request.uri_mut() = uri.parse().unwrap();
            let (status, json_val) = match handle(&request) {
                Ok(json_val) => (200, json_val),
                Err(error) => error.as_response(),
            };
            let body = json_val.to_string().into_bytes();

            response = Some(
                response::Builder::new()
                    .status(status)
                    .header(CONTENT_TYPE, "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .header(CONTENT_LENGTH, body.len().to_string().as_str())
                    .body(body)
                    .unwrap(),
            );
        }))
    };

    match result {
        Ok(()) => response.unwrap(),
        Err(error) => crate::util::log_panic(&request, format!("{:?}", error)),
    }
}

/// Handles routing of incoming requests.
///
/// See the root of the crate for the API layout and
/// [router](router/macro.router.html) for the way this function works.
pub fn handle(request: &cgi::Request) -> GreaseResult<Value> {
    router!(request,
        // authorization
        (POST)   [/login]  => login,
        (GET)    [/logout] => logout,
        // members
        (GET)    [/user] => get_current_user,
        (GET)    [/members/(email: String)?(grades: Option<bool>)?(details: Option<bool>)] => get_member,
        (GET)    [/members/(email: String)/attendance] => get_member_attendance_for_semester,
        (GET)    [/members?(grades: Option<bool>)?(include: Option<String>)] => get_members,
        (POST)   [/members] => new_member,
        (POST)   [/members/confirm] => confirm_for_semester,
        (POST)   [/members/(email: String)/(semester: String)] => update_member_semester,
        (DELETE) [/members/(email: String)/(semester: String)] => mark_member_inactive_for_semester,
        (POST)   [/members] => new_member,
        (POST)   [/members/profile] => update_member_profile,
        (POST)   [/members/(email: String)] => update_member_as_officer,
        (POST)   [/members/(email: String)/login_as] => login_as_member,
        (DELETE) [/members/(email: String)?(confirm: Option<bool>)] => delete_member,
        // events
        (GET)    [/events/(id: i32)?(full: Option<bool>)] => get_event,
        (GET)    [/events?(full: Option<bool>)?(event_types: Option<String>)] => get_events,
        (POST)   [/events] => new_event,
        (POST)   [/events/(id: i32)] => update_event,
        (DELETE) [/events/(id: i32)] => delete_event,
        // event details
        (GET)    [/events/(id: i32)/attendance] => get_attendance,
        (GET)    [/events/(id: i32)/see_whos_attending] => see_whos_attending,
        (GET)    [/events/(id: i32)/attendance/(member: String)] => get_member_attendance,
        (POST)   [/events/(id: i32)/attendance/(member: String)] => update_attendance,
        (POST)   [/events/(id: i32)/rsvp/(attending: bool)] => rsvp_for_event,
        (POST)   [/events/(id: i32)/attendance/excuse_unconfirmed] => excuse_unconfirmed_for_event,
        (GET)    [/events/(id: i32)/carpools] => get_carpools,
        (POST)   [/events/(id: i32)/carpools] => update_carpools,
        (GET)    [/events/(id: i32)/setlist] => get_setlist,
        (POST)   [/events/(id: i32)/setlist] => edit_setlist,
        // absence requests
        (GET)    [/absence_requests] => get_absence_requests,
        (GET)    [/absence_requests/(event_id: i32)] => get_absence_request,
        (GET)    [/absence_requests/(event_id: i32)/is_excused] => member_is_excused,
        (POST)   [/absence_requests/(event_id: i32)/(member: String)/approve] => approve_absence_request,
        (POST)   [/absence_requests/(event_id: i32)/(member: String)/deny] => deny_absence_request,
        (POST)   [/absence_requests/(event_id: i32)] => submit_absence_request,
        // gig requests
        (GET)    [/gig_requests/(id: i32)] => get_gig_request,
        (GET)    [/gig_requests?(all: Option<bool>)] => get_gig_requests,
        (POST)   [/gig_requests] => new_gig_request,
        (POST)   [/gig_requests/(id: i32)/dismiss] => dismiss_gig_request,
        (POST)   [/gig_requests/(id: i32)/reopen] => reopen_gig_request,
        (POST)   [/gig_requests/(id: i32)/create_event] => create_event_from_gig_request,
        // variables
        (GET)    [/variables/(key: String)] => get_variable,
        (POST)   [/variables/(key: String)] => set_variable,
        (DELETE) [/variables/(key: String)] => unset_variable,
        // announcements
        (GET)    [/announcements/(id: i32)] => get_announcement,
        (GET)    [/announcements?(all: Option<bool>)] => get_announcements,
        (POST)   [/announcements] => make_new_announcement,
        (POST)   [/announcements/(id: i32)/archive] => archive_announcement,
        // google docs
        (GET)    [/google_docs/(name: String)] => get_google_doc,
        (GET)    [/google_docs] => get_google_docs,
        (POST)   [/google_docs] => new_google_doc,
        (POST)   [/google_docs/(name: String)] => modify_google_doc,
        (DELETE) [/google_docs/(name: String)] => delete_google_doc,
        // meeting minutes
        (GET)    [/meeting_minutes/(id: i32)] => get_meeting_minutes,
        (GET)    [/meeting_minutes] => get_all_meeting_minutes,
        (POST)   [/meeting_minutes] => new_meeting_minutes,
        (POST)   [/meeting_minutes/(id: i32)] => modify_meeting_minutes,
        (GET)    [/meeting_minutes/(id: i32)/email] => send_minutes_as_email,
        (DELETE) [/meeting_minutes/(id: i32)] => delete_meeting_minutes,
        // uniforms
        (GET)    [/uniforms/(id: i32)] => get_uniform,
        (GET)    [/uniforms] => get_uniforms,
        (POST)   [/uniforms] => new_uniform,
        (POST)   [/uniforms/(id: i32)] => modify_uniform,
        (DELETE) [/uniforms/(id: i32)] => delete_uniform,
        // todos
        (GET)    [/todos] => get_todos,
        (POST)   [/todos] => add_todo_for_members,
        (POST)   [/todos/(id: i32)] => mark_todo_as_complete,
        // songs
        (GET)    [/repertoire/(id: i32)?(details: Option<bool>)] => get_song,
        (GET)    [/repertoire] => get_songs,
        (POST)   [/repertoire] => new_song,
        (POST)   [/repertoire/(id: i32)] => update_song,
        (POST)   [/repertoire/(id: i32)/current] => set_song_as_current,
        (POST)   [/repertoire/(id: i32)/not_current] => set_song_as_not_current,
        (DELETE) [/repertoire/(id: i32)] => delete_song,
        // song links
        (POST)   [/repertoire/(id: i32)/links] => new_song_link,
        (GET)    [/repertoire/links/(id: i32)] => get_song_link,
        (DELETE) [/repertoire/links/(id: i32)] => remove_song_link,
        (POST)   [/repertoire/links/(id: i32)] => update_song_link,
        (POST)   [/repertoire/upload] => upload_file,
        (GET)    [/repertoire/cleanup_files?(confirm: Option<bool>)] => cleanup_song_files,
        // semesters
        (GET)    [/semesters] => get_semesters,
        (GET)    [/semesters/current] => get_current_semester,
        (GET)    [/semesters/(name: String)] => get_semester,
        (POST)   [/semesters] => new_semester,
        (POST)   [/semesters/(name: String)] => edit_semester,
        (POST)   [/semesters/(name: String)/set_current] => set_current_semester,
        (DELETE) [/semesters/(name: String)?(confirm: Option<bool>)] => delete_semester,
        // permissions and roles
        (GET)    [/role_permissions] => get_current_role_permissions,
        (GET)    [/member_roles] => get_current_officers,
        (GET)    [/permissions/(member: String)] => member_permissions,
        (POST)   [/permissions/(position: String)/enable] => add_permission_for_role,
        (POST)   [/permissions/(position: String)/disable] => remove_permission_for_role,
        (POST)   [/roles/add] => add_officership,
        (POST)   [/roles/remove] => remove_officership,
        // fees and transactions
        (GET)    [/fees] => get_fees,
        (POST)   [/fees/(name: String)/(new_amount: i32)] => update_fee_amount,
        (POST)   [/fees/(name: String)/apply] => apply_fee_for_all_active_members,
        (GET)    [/transactions/(member: String)] => get_member_transactions,
        (POST)   [/transactions] => add_transactions,
        // static data
        (GET)    [/static] => static_data,
        (GET)    [/media_types] => get_media_types,
        (GET)    [/permissions] => get_permissions,
        (GET)    [/roles] => get_roles,
        (GET)    [/event_types] => get_event_types,
        (GET)    [/section_types] => get_section_types,
        (GET)    [/transaction_types] => get_transaction_types,
    )
}

/// Returns a basic success message.
///
/// Returns the following with a 200 status code:
/// ```json
/// {
///     "message": "success!"
/// }
/// ```
pub fn basic_success() -> Value {
    json!({ "message": "success!" })
}
