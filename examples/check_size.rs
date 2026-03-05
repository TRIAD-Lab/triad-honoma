use blst::min_pk;
use blst::min_sig;

fn main() {
    println!(
        "min_sig::Signature size: {}",
        std::mem::size_of::<min_sig::Signature>()
    );
    println!(
        "min_pk::Signature size: {}",
        std::mem::size_of::<min_pk::Signature>()
    );

    println!(
        "blst_p1_affine size: {}",
        std::mem::size_of::<blst::blst_p1_affine>()
    );
    println!(
        "blst_p2_affine size: {}",
        std::mem::size_of::<blst::blst_p2_affine>()
    );
}
